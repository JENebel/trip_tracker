use core::{fmt::{self, Debug, Display}, str::FromStr};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{Duration, Timer, WithTimeout};
use embedded_io::Write;
use esp_hal::{gpio::{AnyPin, Level, Output}, uart::{self, AnyUart, AtCmdConfig, Uart, UartRx, UartTx}, Async};

extern crate alloc;
use alloc::{format, string::{String, ToString}, sync::Arc};
use alloc::boxed::Box;

use crate::{byte_buffer::ByteBuffer, debug, error, info, services::comms::connection_buffer::ConnectionBuffer, warn, Service};

use super::{urc_subscriber_set::{URCSubscriberSet, URC_CHANNEL_SIZE}, URCSubscriber, MAX_RESPONSE_LENGTH};

const MINIMUM_AVAILABLE_SPACE: usize = 256;
const BUFFER_SIZE: usize = 1024;

#[derive(Debug, Clone)]
pub enum ATResponse {
    /// The command was successful.
    Ok,
    /// The command was succesful and returned a response.
    Response(String),
    ReadyForInput,
}

impl Display for ATResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ATResponse::Ok => write!(f, "OK"),
            ATResponse::Response(s) => write!(f, "{}", s),
            ATResponse::ReadyForInput => write!(f, ">"),
        }
    }
}

#[derive(Debug)]
pub enum ATErrorType {
    /// An error occurred while sending the command.
    TxError,
    /// An error response was received from the modem.
    AtError,
    NO_CARRIER, // TODO
    NO_DIALTONE, // TODO
    BUSY, // TODO
    NO_ANSWER, // TODO
    CME(String),
    CMS(String), // TODO
    Ip(String),
    NetError(String),
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

    receive_data_buffers: [ConnectionBuffer; 4],
}

#[async_trait::async_trait]
impl Service for ModemService {
    async fn stop(&mut self) {
    }
}

impl Debug for ModemService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Modem Service")
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

        let receive_data_buffers = [
            ConnectionBuffer::new(),
            ConnectionBuffer::new(),
            ConnectionBuffer::new(),
            ConnectionBuffer::new(),
        ];

        spawner.spawn(simcom_monitor(rx, response_signal.clone(), keep_response.clone(), urc_subscriber_set.clone(), receive_data_buffers.clone())).unwrap();

        modem_reset_pin.set_high();
        powerkey_pin.set_high();

        let mut modem = ModemService {
            tx, 
            response_signal, 
            keep_response, 
            modem_reset_pin, 
            powerkey_pin, 
            urc_subscriber_set, 
            receive_data_buffers
        };

        modem.powerkey_pin.set_low();

        let mut a = 0;
        loop {
            let x = modem.send_timeout("ATE0", 5000).await;
            info!("ATE0: {:?}", x);
            if x.is_ok() {
                break;
            }
            a += 1;
            if a > 5 {
                modem.reset().await;
            }
            modem.power_on().await;
        }
        
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
    }

    pub async fn interrogate_timeout(&mut self, command: &str, timeout_ms: u64) -> ATResult {
        self.inner_send(command.as_bytes(), true, timeout_ms).await
    }

    pub async fn send_timeout(&mut self, command: &str, timeout_ms: u64) -> ATResult {
        self.inner_send(command.as_bytes(), false, timeout_ms).await
    }

    /// Defaults to a 10 second timeout
    pub async fn interrogate(&mut self, command: &str) -> ATResult {
        self.inner_send(command.as_bytes(), true, 10000).await
    }

    /// Defaults to a 10 second timeout
    pub async fn send(&mut self, command: &str) -> ATResult {
        self.inner_send(command.as_bytes(), false, 10000).await
    }

    /// Defaults to a 10 second timeout
    async fn send_bytes(&mut self, command: &[u8]) -> ATResult {
        self.inner_send(command, false, 10000).await
    }

    async fn inner_send(&mut self, command: &[u8], keep_result: bool, timeout_ms: u64) -> ATResult {
        let send_closure = async move {
            *self.keep_response.lock().await = keep_result;

            self.tx.write(command).map_err(|_| ATErrorType::TxError)?;
            self.tx.write(&[b'\r']).map_err(|_| ATErrorType::TxError)?;

            self.response_signal.wait().await
        };

        let res = send_closure.with_timeout(Duration::from_millis(timeout_ms)).await;

        let command = core::str::from_utf8(command);
        res.unwrap_or(Err(ATErrorType::Timeout)).map_err(|e| ATError::new(e, command.unwrap_or("")))
    }

    pub async fn interrogate_urc(&mut self, cmd: &str, urc: &'static str, timeout_ms: u64) -> Result<(ATResponse, String), ATError> {
        let sub = self.urc_subscriber_set.add_oneshot(urc).await;
        let id = sub.id;

        let result: Result<(ATResponse, String), ATError> = (async || {
            async fn inner(modem: &mut ModemService, cmd: &str, timeout_ms: u64, sub: URCSubscriber<1>) -> Result<(ATResponse, String), ATError> {
                let res = modem.send_timeout(cmd, timeout_ms).await?;
                let urc_res = sub.channel.receive().await;
                Ok((res, urc_res))
            }
            
            let res = inner(self, cmd, timeout_ms, sub).with_timeout(Duration::from_millis(timeout_ms)).await;

            match res {
                Ok(res) => {res},
                Err(_) => Err(ATError::new(ATErrorType::Timeout, cmd)),
            }
            
        })().await;
        
        // Important cleanup step
        self.urc_subscriber_set.remove_oneshot(id).await;
        result
    }

    pub async fn cip_send_bytes<const CONNECTION: u8>(&mut self, data: &[u8]) -> Result<(), ATError> {
        let cipsend_oneshot = self.urc_subscriber_set.add_oneshot("+CIPSEND").await;

        let result: Result<(), ATError> = (async || {
            match self.send(&format!("AT+CIPSEND={},{}", CONNECTION, data.len())).await? {
                ATResponse::ReadyForInput => {},
                response => return Err(ATError::new(ATErrorType::TxError, &format!("Unexpected response: {:?}. Expected ready for input '>'", response))),
            };

            self.send_bytes(data).await?;

            Ok(())
        })().await;

        let _ = cipsend_oneshot.receive(1000).await;

        self.urc_subscriber_set.remove_oneshot(cipsend_oneshot.id).await;

        result
    }

    pub async fn subscribe_to_urc(&mut self, urc: &'static str) -> URCSubscriber<URC_CHANNEL_SIZE> {
        self.urc_subscriber_set.add(urc).await
    }

    pub fn get_receive_data_buffer(&self, connection_id: usize) -> ConnectionBuffer {
        debug_assert!(connection_id < self.receive_data_buffers.len());
        self.receive_data_buffers[connection_id as usize].clone()
    }
}

#[embassy_executor::task]
async fn simcom_monitor(
    mut rx: UartRx<'static, Async>, 
    response_signal: Arc<Signal<CriticalSectionRawMutex, Result<ATResponse, ATErrorType>>>,
    keep_response: Arc<Mutex<CriticalSectionRawMutex, bool>>,
    urc_subscribers: URCSubscriberSet<8>,
    receive_data_buffers: [ConnectionBuffer; 4],
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

       // println!("Buffer: {:?}", unsafe { core::str::from_utf8_unchecked(buffer.slice()) });
        
        while let Some(message) = try_pop_message(&mut buffer) {
            match message {
                RawMessage::Nmea(_nmea) => {
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
                },
                RawMessage::AtResponse(message) => {
                    let response = if *keep_response.lock().await {
                        let str = core::str::from_utf8(&message[..message.len().min(MAX_RESPONSE_LENGTH)]).unwrap();
                        ATResponse::Response(String::from_str(str).unwrap())
                    } else {
                        ATResponse::Ok
                    };
                    response_signal.signal(Ok(response));
                },
                RawMessage::ReadyForInput => {
                    response_signal.signal(Ok(ATResponse::ReadyForInput));
                },
                RawMessage::URC(message) => {
                    let str = core::str::from_utf8(&message[..message.len().min(MAX_RESPONSE_LENGTH)]).unwrap();
                    let (urc, msg) = match str.split_once(": ") {
                        Some((urc, msg)) => (urc, msg),
                        None => {
                            warn!("Invalid URC: {:?}", str);
                            continue;
                        }
                    };
                    urc_subscribers.send(urc, msg.to_string()).await;
                },
                RawMessage::Error => {
                    response_signal.signal(Err(ATErrorType::AtError));
                },
                RawMessage::CMEError(message) => {
                    let str = core::str::from_utf8(&message[..message.len().min(64)]).unwrap();
                    response_signal.signal(Err(ATErrorType::CME(String::from_str(str).unwrap())));
                },
                RawMessage::CMSError(message) => {
                    let str = core::str::from_utf8(&message[..message.len().min(64)]).unwrap();
                    response_signal.signal(Err(ATErrorType::CMS(String::from_str(str).unwrap())));
                },
                RawMessage::IPError(message) => {
                    let str = core::str::from_utf8(&message[..message.len().min(64)]).unwrap();
                    response_signal.signal(Err(ATErrorType::Ip(String::from_str(str).unwrap())));
                },
                RawMessage::CIPError(message) => {
                    let str = core::str::from_utf8(&message[..message.len().min(64)]).unwrap();
                    response_signal.signal(Err(ATErrorType::Ip(String::from_str(str).unwrap())));
                },
                RawMessage::ReceivedData(connection_id, data) => {
                    let buffer = &receive_data_buffers[connection_id as usize];
                    buffer.write(data).await;
                },
            }
        }

        buffer.shift_back();

        if buffer.remaining_capacity() < MINIMUM_AVAILABLE_SPACE {
            warn!("Not enough capacity, clearing buffer");
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

#[derive(Debug)]
enum RawMessage<'a> {
    Nmea(&'a [u8]),
    AtResponse(&'a [u8]),
    URC(&'a [u8]),
    Error,
    CMEError(&'a [u8]),
    CMSError(&'a [u8]),
    IPError(&'a [u8]),
    CIPError(&'a [u8]),
    ReadyForInput,

    /// RECV FROM message, for example TCP/IP data. Contains the Connection ID and the data.
    ReceivedData(u8, &'a [u8]),
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

    if trimmed == b">" {
        buffer.pop(leading_ws + 1);
        return Some(RawMessage::ReadyForInput);
    }

    for i in leading_ws..buffer.len() + 1 {
        if trimmed.starts_with(NMEA_PREFIX) {
            if buffer.slice()[leading_ws..i].ends_with(NMEA_TERMINATOR) {
                return Some(RawMessage::Nmea(buffer.pop(i).trim_ascii()));
            }
        }
        
        else if trimmed.starts_with(URC_PREFIX) {
            if buffer.slice()[leading_ws..i].ends_with(URC_TERMINATOR) {
                if trimmed.starts_with(b"+RECEIVE") {
                    // Special case with 2 lines of data, the second depending in length on the first
                    // +RECEIVE,0,16\r\n[DATA BYTES]
                    
                    // Step 1: Get the length and ensure the buffer contains the entire message
                    let data_len = core::str::from_utf8(&trimmed[11..i-2-leading_ws]).unwrap();
                    let data_len = data_len.parse::<usize>().unwrap();
                    if buffer.len() < i + data_len {
                        return None;
                    }

                    let connection_id = core::str::from_utf8(&trimmed[9..10]).unwrap().parse::<u8>().unwrap();
                    
                    buffer.pop(i);
                    let data = buffer.pop(data_len);

                    return Some(RawMessage::ReceivedData(connection_id, data));
                }

                let unsolicited = buffer.pop(i).trim_ascii();
                
                if unsolicited.starts_with(b"+CME ERROR: ") {
                    return Some(RawMessage::CMEError(&unsolicited[12..]));
                }

                if unsolicited.starts_with(b"+CMS ERROR: ") {
                    return Some(RawMessage::CMSError(&unsolicited[12..]));
                }

                if unsolicited.starts_with(b"+IP ERROR: ") {
                    return Some(RawMessage::IPError(&unsolicited[11..]));
                }

                if unsolicited.starts_with(b"+CIPERROR: ") {
                    return Some(RawMessage::CIPError(&unsolicited[11..]));
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