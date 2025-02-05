
use core::{fmt::{self, Display}, str::FromStr};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::Timer;
use embedded_io::Write;
use embedded_io_async::Read;
use esp_hal::{dma::ReadBuffer, gpio::AnyPin, peripherals::Interrupt, uart::{self, AnyUart, AtCmdConfig, Uart, UartRx, UartTx}, xtensa_lx::mutex::CriticalSectionSpinLockMutex, Async, Cpu};
use esp_println::{print, println};
use heapless::{HistoryBuffer, String};

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
    /// An error occurred while waiting for the response.
    RxError,
    /// An error occurred while sending the command.
    TxError,
}

impl Display for ATError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ATError::AtError => write!(f, "AT Error"),
            ATError::RxError => write!(f, "RX Error"),
            ATError::TxError => write!(f, "TX Error"),
        }
    }
}

pub type ATResult = Result<ATResponse, ATError>;

pub static SIM7670G: Mutex<CriticalSectionRawMutex, Option<Simcom7670>> = Mutex::new(None);

static RESPONSE_SIGNAL: Signal<CriticalSectionRawMutex, ATResult> = Signal::new();
static KEEP_RESPONSE: Signal<CriticalSectionRawMutex, bool> = Signal::new();

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
            rx_fifo_full_threshold: 120,
            ..Default::default()
        };
        
        let mut uart = Uart::new_with_config(uart, config, rx, tx).unwrap().into_async();
        uart.set_at_cmd(AtCmdConfig::new(None, None, None, 13, None));

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

        self.send("AT+CGNSSTST=0").await.unwrap();

        self.send("AT+CGNSSMODE=15").await.unwrap(); // GPS + GLONASS + GALILEO + BDS

        // NMEA configuration
        self.send("AT+CGNSSNMEA=1,0,0,0,0,0,0,0").await.unwrap();

        //self.send("AT+CGNSSNMEARATE=1010").await?;

        self.send("AT+CGNSSPORTSWITCH=0,1").await.unwrap();
    
        Ok(ATResponse::Ok)
    }

    async fn inner_send(&mut self, cmd: &str, keep_result: bool) -> ATResult {
        println!("{}", cmd);
        KEEP_RESPONSE.signal(keep_result);

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

#[embassy_executor::task]
pub async fn start_reader(mut rx: UartRx<'static, Async>) {
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut head = 0;

    // Clear the FIFO before reading
    rx.drain_fifo(&mut buffer);

    loop {
        match rx.read(&mut buffer[head..]).await {
            Ok(n) => {
                head += n;
                if let Some(message) = try_get_message(&buffer[..head]) {
                    if message.starts_with(b"$") {
                        let str = core::str::from_utf8(message).unwrap();
                        println!("NMEA:\n{}\n", str.trim());
                        let length = message.len();
                        head -= length;
                        buffer.rotate_left(length);
                    } else {
                        let str = core::str::from_utf8(&buffer[..head]).unwrap();
                        let keep_result = KEEP_RESPONSE.wait().await;

                        println!("AT:\n{}\n", str.trim());

                        // Spare the allocation if we don't need to keep the result
                        let response = if keep_result {
                            ATResponse::Response(String::from_str(str).unwrap())
                        } else {
                            ATResponse::Ok
                        };

                        let ok = str.trim().ends_with("OK");

                        if !ok {
                            RESPONSE_SIGNAL.signal(Err(ATError::AtError));
                        } else {
                            RESPONSE_SIGNAL.signal(Ok(response));
                        }

                        head = 0;
                    }
                }
            }
            Err(e) => println!("RX Error: {:?}", e),
        }
    }
}

/// This returns a slice of the buffer that contains the first message.
/// There could be more content in the buffer, and the caller should handle this so as to not lose it.
fn try_get_message(buffer: &[u8]) -> Option<&[u8]> {
    for i in 0..buffer.len() - 1 {
        if buffer[i] == 13 && buffer[i + 1] == 10 {
            return Some(&buffer[..i + 2]);
        }
    }
    
    None
}