use core::{fmt::{self, Display}, str::FromStr};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embedded_io::Write;
use esp_hal::{gpio::AnyPin, uart::{self, AnyUart, AtCmdConfig, Uart, UartRx, UartTx}, Async};
use esp_println::println;
use heapless::String;
use nmea::{Nmea, SentenceType};

use crate::byte_buffer::ByteBuffer;

const BUFFER_SIZE: usize = 1024;

#[derive(Debug)]
pub enum ATResponse {
    /// The command was successful.
    Ok,
    /// The command was succesful and returned a response.
    Response(String<256>),
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
        match self {
            ATError::AtError => write!(f, "AT Error"),
            ATError::TxError => write!(f, "TX Error"),
        }
    }
}

pub type ATResult = Result<ATResponse, ATError>;

pub static SIM7670G: Mutex<CriticalSectionRawMutex, Option<Simcom7670>> = Mutex::new(None);

static RESPONSE_SIGNAL: Signal<CriticalSectionRawMutex, ATResult> = Signal::new();
static KEEP_RESPONSE: Mutex<CriticalSectionRawMutex, bool> = Mutex::new(false);

pub struct Simcom7670 {
    tx: UartTx<'static, Async>,
}

impl Simcom7670 {
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

        SIM7670G.lock().await.replace(Simcom7670 { tx });

        SIM7670G.lock().await.as_mut().unwrap().send("ATE0").await.unwrap();
    }

    pub async fn enable_gnss(&mut self) -> ATResult {
        // Power on GNSS
        self.send("AT+CGDRT=4,1").await.unwrap();
        self.send("AT+CGSETV=4,1").await.unwrap();
        self.send("AT+CGNSSPWR=1").await.unwrap();

        self.send("AT+CGNSSTST=1").await.unwrap();

        self.send("AT+CGNSSMODE=15").await.unwrap(); // GPS + GLONASS + GALILEO + BDS

        // NMEA configuration
        self.send("AT+CGNSSNMEA=1,0,0,0,0,0,0,0").await.unwrap();

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
}

fn handle_nmea_sentence(sentence: &str) {
    let mut nmea = Nmea::create_for_navigation(&[SentenceType::GGA]).unwrap();
    match nmea.parse(sentence) {
        Ok(nmea::SentenceType::GGA) => {
            if let Some(sats) = nmea.num_of_fix_satellites {
                let x = nmea.latitude;
                let y = nmea.longitude;
                if x.is_some() && y.is_some() {
                    esp_println::println!("Sats: {sats}, pos: {}, {}", x.unwrap(), y.unwrap());
                }
            }
        }
        _ => (),
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
                    let str = core::str::from_utf8(nmea).unwrap();
                    handle_nmea_sentence(str);
                },
                RawResponse::Ok(message) => {
                    let keep_result = *KEEP_RESPONSE.lock().await;
                    let response = if keep_result {
                        let str = core::str::from_utf8(message).unwrap();
                        ATResponse::Response(String::from_str(str).unwrap())
                    } else {
                        ATResponse::Ok
                    };
                    RESPONSE_SIGNAL.signal(Ok(response));
                },
                RawResponse::Err => {
                    RESPONSE_SIGNAL.signal(Err(ATError::AtError));
                },
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