use core::{fmt::{self, Display}, str::FromStr};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::{Mutex, MutexGuard}, once_lock::OnceLock, signal::Signal};
use embassy_time::Timer;
use embedded_io::Write;
use esp_hal::{gpio::AnyPin, uart::{self, AnyUart, AtCmdConfig, Uart, UartRx, UartTx}, Async};
use esp_println::println;
use heapless::String;

use crate::{byte_buffer::ByteBuffer, gnss::NMEAChannel};

const BUFFER_SIZE: usize = 1024;
const MAX_RESPONSE_LENGTH: usize = 256;

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

        spawner.spawn(start_reader(rx)).unwrap();

        MODEM.init(Mutex::new(SimComModem { tx })).map_err(|_|()).unwrap();

        Self::aqcuire().await.send("ATE0").await.unwrap();
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
        println!("{}", cmd);
        *KEEP_RESPONSE.lock().await = keep_result;

        self.tx.write(cmd.as_bytes()).map_err(|_| ATError::TxError)?;
        self.tx.write(&[b'\r']).map_err(|_| ATError::TxError)?;

        RESPONSE_SIGNAL.wait().await
    }

    pub async fn interrogate(&mut self, cmd: &str) -> ATResult {
        self.inner_send(cmd, true).await
    }

    pub async fn send(&mut self, cmd: &str) -> ATResult {
        self.inner_send(cmd, false).await
    }

    pub fn get_nmea_channel() -> &'static NMEAChannel {
        &NMEA_QUEUE
    }
}

#[embassy_executor::task]
pub async fn start_reader(mut rx: UartRx<'static, Async>) {
    let mut buffer = ByteBuffer::<BUFFER_SIZE>::new();

    loop {
        match rx.read_async(buffer.remaining_space_mut()).await {
            Ok(n) => {
                buffer.claim(n);
            }
            Err(e) => println!("RX Error: {:?}", e),
        }
        
        while let Some(response) = try_pop_message(&mut buffer) {
            match response {
                RawResponse::Nmea(nmea) => {
                    let str = core::str::from_utf8(nmea.trim_ascii()).unwrap();
                    let string = String::from_str(str).unwrap(); 

                    if !NMEA_QUEUE.is_full() {
                        NMEA_QUEUE.send(string).await;
                    } else {
                        println!("NMEA queue full, dropping message");
                    }
                }
                RawResponse::Ok(message) => {
                    let keep_result = *KEEP_RESPONSE.lock().await;
                    let response = if keep_result {
                        let str = core::str::from_utf8(&message[..message.len().min(MAX_RESPONSE_LENGTH)]).unwrap();
                        ATResponse::Response(String::from_str(str).unwrap())
                    } else {
                        ATResponse::Ok
                    };
                    RESPONSE_SIGNAL.signal(Ok(response));
                }
                RawResponse::Err => {
                    RESPONSE_SIGNAL.signal(Err(ATError::AtError));
                }
            }
        }
        buffer.shift_back();
    }
}

const AT_OK_TERMINATOR: &[u8] = b"OK\r\n";
const AT_ERR_TERMINATOR: &[u8] = b"ERROR\r\n";
const NMEA_TERMINATOR: &[u8] = b"\r\n";
const NMEA_PREFIX: &[u8] = b"$";

#[derive(Debug)]
enum RawResponse<'a> {
    Nmea(&'a [u8]),
    Ok(&'a [u8]),
    Err,
}

fn try_pop_message<const SIZE: usize> (buffer: &mut ByteBuffer<SIZE>) -> Option<RawResponse> {
    let trimmed = buffer.slice().trim_ascii_start();
    if trimmed.is_empty() {
        return None;
    }

    let leading_ws = buffer.len() - trimmed.len();

    for i in leading_ws..buffer.len() {
        if trimmed.starts_with(NMEA_PREFIX) {
            if buffer.slice()[..=i].ends_with(NMEA_TERMINATOR) {
                return Some(RawResponse::Nmea(buffer.pop(i).trim_ascii()));
            }
        } else if buffer.slice()[..=i].ends_with(AT_OK_TERMINATOR) {
            return Some(RawResponse::Ok(buffer.pop(i).trim_ascii()));
        } else if buffer.slice()[..=i].ends_with(AT_ERR_TERMINATOR) {
            buffer.pop(i);
            return Some(RawResponse::Err);
        }
    }

    None
}