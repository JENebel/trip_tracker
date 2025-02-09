use core::{fmt::{self, Display}, str::FromStr};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::{Mutex, MutexGuard}, once_lock::OnceLock, signal::Signal};
use embassy_time::{Duration, TimeoutError};
use embedded_io::Write;
use esp_hal::{gpio::AnyPin, uart::{self, AnyUart, AtCmdConfig, Uart, UartRx, UartTx}, Async};
use esp_println::println;
use heapless::String;

use crate::{byte_buffer::ByteBuffer, gnss::NMEAChannel};

const MINIMUM_AVAILABLE_SPACE: usize = 256;
const BUFFER_SIZE: usize = 1024;
const MAX_RESPONSE_LENGTH: usize = 256;
pub const MAX_NMEA_LENGTH: usize = 103;

#[derive(Debug)]
pub enum ATResponse {
    /// The command was successful.
    Ok,
    /// The command was succesful and returned a response.
    Response(String<MAX_RESPONSE_LENGTH>),
}

impl Display for ATResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ATResponse::Ok => write!(f, "OK"),
            ATResponse::Response(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug)]
pub enum ATError {
    /// An error response was received from the modem.
    AtError,
    /// An error occurred while sending the command.
    TxError,
    Timeout,
}

impl Display for ATError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub type ATResult = Result<ATResponse, ATError>;

pub static MODEM: OnceLock<Mutex<CriticalSectionRawMutex, SimComModem>> = OnceLock::new();

static RESPONSE_SIGNAL: Signal<CriticalSectionRawMutex, ATResult> = Signal::new();
static KEEP_RESPONSE: Mutex<CriticalSectionRawMutex, bool> = Mutex::new(false);
static NMEA_QUEUE: NMEAChannel = Channel::new();

pub struct SimComModem {
    tx: UartTx<'static, Async>,
}

impl SimComModem {
    pub async fn initialize(
        spawner: &embassy_executor::Spawner,
        uart: esp_hal::peripheral::PeripheralRef<'static, AnyUart>, 
        rx: esp_hal::peripheral::PeripheralRef<'static, AnyPin>, 
        tx: esp_hal::peripheral::PeripheralRef<'static, AnyPin>) {

        let config = uart::Config {
            baudrate: 115200,
            data_bits: uart::DataBits::DataBits8,
            parity: uart::Parity::ParityNone,
            ..Default::default()
        };
        
        let mut uart = Uart::new_with_config(uart, config, rx, tx).unwrap().into_async();
        uart.set_at_cmd(AtCmdConfig::new(None, None, None, b'\r', None));

        let (rx, tx) = uart.split();

        spawner.spawn(simcom_monitor(rx)).unwrap();

        let mut modem = SimComModem { tx };
        modem.send("ATE1").await.unwrap();

        MODEM.init(Mutex::new(modem)).map_err(|_|()).unwrap();
    }

    pub async fn aqcuire() -> MutexGuard<'static, CriticalSectionRawMutex, SimComModem> {
        if !MODEM.is_set(){
            panic!("Modem not initialized");
        }
        MODEM.get().await.lock().await
    }

    pub async fn enable_gnss(&mut self) -> ATResult {
        // Power on GNSS
        println!("Enabling GNSS");
    
        self.send("AT+CGNSSPWR=1").await.unwrap();

        self.send("AT+CGDRT=4,1").await.unwrap();
        self.send("AT+CGSETV=4,1").await.unwrap();

        self.send("AT+CGNSSMODE=15").await.unwrap(); // GPS + GLONASS + GALILEO + BDS

        // NMEA configuration. GGA, VTG, ZDA are enabled.
        self.send("AT+CGNSSNMEA=1,0,0,0,0,1,1,0").await.unwrap();

        self.send("AT+CGNSSTST=1").await.unwrap();

        self.send("AT+CGNSSPORTSWITCH=0,1").await.unwrap();
    
        Ok(ATResponse::Ok)
    }

    async fn inner_send(&mut self, cmd: &str, keep_result: bool) -> ATResult {
        *KEEP_RESPONSE.lock().await = keep_result;

        self.tx.write(cmd.as_bytes()).map_err(|_| ATError::TxError)?;
        self.tx.write(&[b'\r']).map_err(|_| ATError::TxError)?;

        RESPONSE_SIGNAL.wait().await
    }

    pub async fn interrogate(&mut self, cmd: &str) -> ATResult {
        match embassy_time::with_timeout(Duration::from_secs(1), self.inner_send(cmd, true)).await {
            Ok(result) => result,
            Err(TimeoutError) => Err(ATError::AtError),
        }
    }

    pub async fn send(&mut self, cmd: &str) -> ATResult {
        self.inner_send(cmd, false).await
    }

    pub fn get_nmea_channel() -> &'static NMEAChannel {
        &NMEA_QUEUE
    }
}

#[embassy_executor::task]
async fn simcom_monitor(mut rx: UartRx<'static, Async>) {
    let mut buffer = ByteBuffer::<BUFFER_SIZE>::new();

    loop {
        match rx.read_async(buffer.remaining_space_mut()).await {
            Ok(n) => {
                buffer.claim(n);

                while buffer.len() > 0 && !buffer.slice().starts_with(AT_PREFIX) && !buffer.slice().starts_with(NMEA_PREFIX) {
                    println!("Discarding until AT or NMEA prefix: {:?}", core::str::from_utf8(buffer.slice()).unwrap());
                    discard_until_separator(&mut buffer);
                }
            }
            Err(e) => match e {
                uart::Error::InvalidArgument => panic!("Not enough space in buffer: {:?}", core::str::from_utf8(buffer.slice()).unwrap()),
                uart::Error::RxFifoOvf => {
                    println!("RX FIFO overflow");
                },
                uart::Error::RxGlitchDetected => println!("RX glitch detected"),
                uart::Error::RxFrameError => println!("RX frame error"),
                uart::Error::RxParityError => println!("RX parity error"),
            }
        }
        
        while let Some(message) = try_pop_message(&mut buffer) {
            match message {
                RawMessage::Nmea(nmea) => {
                    let trimmed = nmea.trim_ascii();
                    if trimmed.starts_with(PAIR_MESSAGE_PREFIX) {
                        // Early filter away PAIR messages like "$PAIR001,066,0*3B". No idea what these are, but they are unwanted
                         continue;
                    }

                    let mut arr: [u8; MAX_NMEA_LENGTH] = [0; MAX_NMEA_LENGTH];
                    let len = trimmed.len().min(MAX_NMEA_LENGTH);
                    arr[..len].clone_from_slice(&trimmed[..len]);

                    if !NMEA_QUEUE.is_full() {
                        NMEA_QUEUE.send((arr, trimmed.len())).await;
                    } else {
                        println!("NMEA queue full, dropping message");
                    }
                }
                RawMessage::AtResponse(message) => {
                    let keep_result = *KEEP_RESPONSE.lock().await;
                    let response = if keep_result {
                        let str = core::str::from_utf8(&message[..message.len().min(MAX_RESPONSE_LENGTH)]).unwrap();
                        ATResponse::Response(String::from_str(str).unwrap())
                    } else {
                        ATResponse::Ok
                    };
                    RESPONSE_SIGNAL.signal(Ok(response));
                }
                RawMessage::Err => {
                    RESPONSE_SIGNAL.signal(Err(ATError::AtError));
                }
            }
        }
        buffer.shift_back();

        if buffer.remaining_capacity() < MINIMUM_AVAILABLE_SPACE {
            println!("Not enough capacity, clearing buffer: {:?}", core::str::from_utf8(buffer.slice()).unwrap());
            discard_until_separator(&mut buffer);
        }
    }
}

const AT_OK_TERMINATOR: &[u8] = b"OK\r\n";
const AT_ERR_TERMINATOR: &[u8] = b"ERROR\r\n";
const NMEA_TERMINATOR: &[u8] = b"\r\n";
const NMEA_PREFIX: &[u8] = b"$";
const AT_PREFIX: &[u8] = b"AT";
const PAIR_MESSAGE_PREFIX: &[u8] = b"$PAIR";

#[derive(Debug)]
enum RawMessage<'a> {
    Nmea(&'a [u8]),
    AtResponse(&'a [u8]),
    Err,
}

fn discard_until_separator<const SIZE: usize> (buffer: &mut ByteBuffer<SIZE>) {
    for i in 0..buffer.len() {
        if buffer.slice()[..i].ends_with(NMEA_TERMINATOR) {
            buffer.pop(i);
            println!("Discarded {} bytes", i);
            return;
        }
    }

    println!("Discarded {} bytes", buffer.len());
    buffer.clear();
}

// Todo: improve this functions performance and readability
fn try_pop_message<const SIZE: usize> (buffer: &mut ByteBuffer<SIZE>) -> Option<RawMessage> {
    let trimmed = buffer.slice().trim_ascii_start();
    if trimmed.is_empty() {
        return None;
    }

    let leading_ws = buffer.len() - trimmed.len();

    for i in leading_ws..buffer.len() {
        if trimmed.starts_with(NMEA_PREFIX) {
            if buffer.slice()[..i].ends_with(NMEA_TERMINATOR) {
                return Some(RawMessage::Nmea(buffer.pop(i).trim_ascii()));
            }
        } else if buffer.slice()[..i].ends_with(AT_OK_TERMINATOR) {
            return Some(RawMessage::AtResponse(buffer.pop(i).trim_ascii()));
        } else if buffer.slice()[..i].ends_with(AT_ERR_TERMINATOR) {
            buffer.pop(i);
            return Some(RawMessage::Err);
        }
    }

    None
}