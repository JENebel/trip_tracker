use core::{fmt::{self, Debug, Display}, str::FromStr};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{Duration, Timer, WithTimeout};
use embedded_io::Write;
use esp_hal::{gpio::{AnyPin, Level, Output}, uart::{self, AnyUart, AtCmdConfig, Uart, UartRx, UartTx}, Async};

extern crate alloc;
use alloc::{string::{String, ToString}, sync::Arc};
use alloc::boxed::Box;

use crate::{byte_buffer::ByteBuffer, debug, error, info, warn, Service};

use super::{urc_subscriber_set::{URCSubscriberSet, URC_CHANNEL_SIZE}, URCSubscriber, MAX_RESPONSE_LENGTH};

const MINIMUM_AVAILABLE_SPACE: usize = 256;
const BUFFER_SIZE: usize = 1024;

#[derive(Debug, Clone)]
pub enum ATResponse {
    /// The command was successful.
    Ok,
    /// The command was succesful and returned a response.
    Response(String),
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
pub enum ATErrorType {
    /// An error occurred while sending the command.
    TxError,
    /// An error response was received from the modem.
    Error,
    NO_CARRIER, // TODO
    NO_DIALTONE, // TODO
    BUSY, // TODO
    NO_ANSWER, // TODO
    CME(String),
    CMS(String), // TODO
    Timeout,
}

#[derive(Debug)]
pub struct ATError {
    error_type: ATErrorType,
    command: String,
}

impl ATError {
    pub fn new(error_type: ATErrorType, command: &str) -> Self {
        ATError {
            error_type,
            command: String::from_str(command).unwrap(),
        }
    }
}

impl Display for ATError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub type ATResult = Result<ATResponse, ATError>;

pub struct ModemService {
    tx: UartTx<'static, Async>,
    response_signal: Arc<Signal<CriticalSectionRawMutex, Result<ATResponse, ATErrorType>>>,
    keep_response: Arc<Mutex<CriticalSectionRawMutex, bool>>,

    urc_subscriber_set: URCSubscriberSet<8>,

    modem_reset_pin: Output<'static>,
    powerkey_pin: Output<'static>,
}

#[async_trait::async_trait]
impl Service for ModemService {
    async fn start(&mut self) {
        debug!("Modem service started!");
    }

    async fn stop(&mut self) {
        debug!("Modem service stopped!");
    }
}

impl Debug for ModemService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModemService {{ }}")
    }
}

impl ModemService {
    pub async fn initialize(
        spawner: &embassy_executor::Spawner,
        uart: esp_hal::peripheral::PeripheralRef<'static, AnyUart>, 
        rx: esp_hal::peripheral::PeripheralRef<'static, AnyPin>, 
        tx: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        modem_reset_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        powerkey_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
    ) -> Self {

        let config = uart::Config {
            baudrate: 115200,
            data_bits: uart::DataBits::DataBits8,
            parity: uart::Parity::ParityNone,
            ..Default::default()
        };
        
        let mut uart = Uart::new_with_config(uart, config, rx, tx).unwrap().into_async();
        uart.set_at_cmd(AtCmdConfig::new(None, None, None, b'\r', None));

        let (rx, tx) = uart.split();
    
        let response_signal = Arc::new(Signal::new());
        let keep_response = Arc::new(Mutex::new(false));

        let mut modem_reset_pin = Output::new(modem_reset_pin, Level::High);
        let mut powerkey_pin = Output::new(powerkey_pin, Level::High);

        let urc_subscriber_set = URCSubscriberSet::new();
        spawner.spawn(simcom_monitor(rx, response_signal.clone(), keep_response.clone(), urc_subscriber_set.clone())).unwrap();

        modem_reset_pin.set_high();
        powerkey_pin.set_high();

        let mut modem = ModemService { tx, response_signal, keep_response, modem_reset_pin, powerkey_pin, urc_subscriber_set };

        modem.power_on().await;

        //modem.reset().await;

        modem.send_timeout("ATE0", 5000).await.unwrap();

        modem
    }

    async fn power_on(&mut self) {
        self.powerkey_pin.set_low();
        Timer::after_millis(100).await;
        self.powerkey_pin.set_high();
        Timer::after_millis(1000).await;
        self.powerkey_pin.set_low();
    }

    async fn reset(&mut self) {
        info!("Resetting modem...");
        self.modem_reset_pin.set_high();
        Timer::after_millis(100).await;
        self.modem_reset_pin.set_low();
        Timer::after_millis(2600).await;
        self.modem_reset_pin.set_high();
    
        // debug!("PWRKEY pin cycle...");
        self.powerkey_pin.set_low();
        Timer::after_millis(100).await;
        self.powerkey_pin.set_high();
        Timer::after_millis(1000).await;
        self.powerkey_pin.set_low();
        
        //debug!("Modem reset complete!");
    }

    pub async fn interrogate_timeout(&mut self, command: &str, timeout_ms: u64) -> ATResult {
        self.inner_send(command, true, timeout_ms).await
    }

    pub async fn send_timeout(&mut self, command: &str, timeout_ms: u64) -> ATResult {
        self.inner_send(command, false, timeout_ms).await
    }

    /// Defaults to a 10 second timeout
    pub async fn interrogate(&mut self, command: &str) -> ATResult {
        self.inner_send(command, true, 10000).await
    }

    /// Defaults to a 10 second timeout
    pub async fn send(&mut self, command: &str) -> ATResult {
        self.inner_send(command, false, 10000).await
    }

    async fn inner_send(&mut self, command: &str, keep_result: bool, timeout_ms: u64) -> ATResult {
        let send_closure = async move {
            *self.keep_response.lock().await = keep_result;

            self.tx.write(command.as_bytes()).map_err(|_| ATErrorType::TxError)?;
            self.tx.write(&[b'\r']).map_err(|_| ATErrorType::TxError)?;

            self.response_signal.wait().await
        };

        let res = send_closure.with_timeout(Duration::from_millis(timeout_ms)).await;

        res.unwrap_or(Err(ATErrorType::Timeout)).map_err(|e| ATError::new(e, command))
    }

    pub async fn interrogate_urc(&mut self, cmd: &str, urc: &'static str, timeout_ms: u64) -> Result<String, ATError> {
        let sub = self.urc_subscriber_set.add_oneshot(urc).await;
        let id = sub.id;

        async fn inner(modem: &mut ModemService, cmd: &str, timeout_ms: u64, sub: URCSubscriber<1>) -> Result<String, ATError> {
            modem.send_timeout(cmd, timeout_ms).await?;
            Ok(sub.channel.receive().await)
        }
        
        let res = inner(self, cmd, timeout_ms, sub).with_timeout(Duration::from_millis(timeout_ms)).await;
        
        // Important cleanup step
        self.urc_subscriber_set.remove_oneshot(id).await;

        match res {
            Ok(res) => {res},
            Err(_) => Err(ATError::new(ATErrorType::Timeout, cmd)),
        }
    }

    pub async fn subscribe_to_urc(&mut self, urc: &'static str) -> URCSubscriber<URC_CHANNEL_SIZE> {
        self.urc_subscriber_set.add(urc).await
    }
}

#[embassy_executor::task]
async fn simcom_monitor(
    mut rx: UartRx<'static, Async>, 
    response_signal: Arc<Signal<CriticalSectionRawMutex, Result<ATResponse, ATErrorType>>>,
    keep_response: Arc<Mutex<CriticalSectionRawMutex, bool>>,
    urc_subscribers: URCSubscriberSet<8>,
) {
    let mut buffer = ByteBuffer::<BUFFER_SIZE>::new();

    loop {
        match rx.read_async(buffer.remaining_space_mut()).await {
            Ok(n) => {
                buffer.claim(n);
            }
            Err(e) => match e {
                uart::Error::InvalidArgument => panic!("Not enough space in buffer: {:?}", core::str::from_utf8(buffer.slice()).unwrap()),
                uart::Error::RxFifoOvf => {
                    error!("RX FIFO overflow");
                },
                uart::Error::RxGlitchDetected => error!("RX glitch detected"),
                uart::Error::RxFrameError => error!("RX frame error"),
                uart::Error::RxParityError => error!("RX parity error"),
            }
        }
        
        while let Some(message) = try_pop_message(&mut buffer) {
            match message {
                RawMessage::Nmea(nmea) => {
                    /*let trimmed = nmea.trim_ascii();
                    if trimmed.starts_with(PAIR_MESSAGE_PREFIX) {
                        // Early filter away PAIR messages like "$PAIR001,066,0*3B". No idea what these are, but they are unwanted
                         continue;
                    }

                    let mut arr: [u8; MAX_NMEA_LENGTH] = [0; MAX_NMEA_LENGTH];
                    let len = trimmed.len().min(MAX_NMEA_LENGTH);
                    arr[..len].clone_from_slice(&trimmed[..len]);

                    if trimmed.len() > MAX_NMEA_LENGTH {
                        println!("NMEA message too long, truncating: {:?}", core::str::from_utf8(&trimmed).unwrap());
                    }

                    if NMEA_QUEUE.is_full() {
                        println!("NMEA queue full, discarding message");
                        let _ = NMEA_QUEUE.try_receive();
                    }
                    
                    NMEA_QUEUE.send((arr, len)).await;*/
                }
                RawMessage::AtResponse(message) => {
                    let response = if *keep_response.lock().await {
                        let str = core::str::from_utf8(&message[..message.len().min(MAX_RESPONSE_LENGTH)]).unwrap();
                        ATResponse::Response(String::from_str(str).unwrap())
                    } else {
                        ATResponse::Ok
                    };
                    response_signal.signal(Ok(response));
                }
                RawMessage::URC(message) => {
                    let str = core::str::from_utf8(&message[..message.len().min(MAX_RESPONSE_LENGTH)]).unwrap();
                    debug!("URC: {:?}", str);
                    let (urc, msg) = str.split_once(": ").unwrap();
                    urc_subscribers.send(urc, msg.to_string()).await;
                }
                RawMessage::Error => {
                    response_signal.signal(Err(ATErrorType::Error));
                },
                RawMessage::CMEError(message) => {
                    let str = core::str::from_utf8(&message[..message.len().min(64)]).unwrap();
                    response_signal.signal(Err(ATErrorType::CME(String::from_str(str).unwrap())));
                },
                RawMessage::CMSError(message) => {
                    let str = core::str::from_utf8(&message[..message.len().min(64)]).unwrap();
                    response_signal.signal(Err(ATErrorType::CMS(String::from_str(str).unwrap())));
                }
            }
        }

        buffer.shift_back();

        if buffer.remaining_capacity() < MINIMUM_AVAILABLE_SPACE {
            warn!("Not enough capacity, clearing buffer: {:?}", core::str::from_utf8(buffer.slice()).unwrap());
            discard_until_separator(&mut buffer).await;
        }
    }
}

const AT_OK_TERMINATOR: &[u8] = b"OK\r\n";
const AT_ERR_TERMINATOR: &[u8] = b"ERROR\r\n";
const NMEA_TERMINATOR: &[u8] = b"\r\n";
const NMEA_PREFIX: &[u8] = b"$";
const URC_TERMINATOR: &[u8] = b"\n";
const URC_PREFIX: &[u8] = b"+";
const AT_PREFIX: &[u8] = b"AT";

#[derive(Debug)]
enum RawMessage<'a> {
    Nmea(&'a [u8]),
    AtResponse(&'a [u8]),
    URC(&'a [u8]),
    Error,
    CMEError(&'a [u8]),
    CMSError(&'a [u8]),
}

async fn discard_until_separator<const SIZE: usize> (buffer: &mut ByteBuffer<SIZE>) {
    for i in 0..buffer.len() {
        if buffer.slice()[..i].ends_with(NMEA_TERMINATOR) {
            buffer.pop(i);
            debug!("Discarded {} bytes", i);
            return;
        }
    }

    debug!("Discarded {} bytes", buffer.len());
    buffer.clear();
}

// Todo: improve this functions performance and readability
fn try_pop_message<const SIZE: usize> (buffer: &mut ByteBuffer<SIZE>) -> Option<RawMessage> {
    let trimmed = buffer.slice().trim_ascii_start();
    if trimmed.is_empty() {
        return None;
    }

    let leading_ws = buffer.len() - trimmed.len();

    for i in leading_ws..buffer.len() + 1 {
        if trimmed.starts_with(NMEA_PREFIX) {
            if buffer.slice()[leading_ws..i].ends_with(NMEA_TERMINATOR) {
                return Some(RawMessage::Nmea(buffer.pop(i).trim_ascii()));
            }
        }
        
        else if trimmed.starts_with(URC_PREFIX) {
            if buffer.slice()[leading_ws..i].ends_with(URC_TERMINATOR) {
                let unsolicited = buffer.pop(i).trim_ascii();
                
                if unsolicited.starts_with(b"+CME ERROR: ") {
                    return Some(RawMessage::CMEError(&unsolicited[12..]));
                }

                if unsolicited.starts_with(b"+CMS ERROR: ") {
                    return Some(RawMessage::CMSError(&unsolicited[12..]));
                }

                return Some(RawMessage::URC(unsolicited));
            }
        }

        if buffer.slice()[leading_ws..i].ends_with(AT_OK_TERMINATOR) {
            return Some(RawMessage::AtResponse(buffer.pop(i).trim_ascii()));
        } else if buffer.slice()[leading_ws..i].ends_with(AT_ERR_TERMINATOR) {
            buffer.pop(i);
            return Some(RawMessage::Error);
        }
    }

    None
}