#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, signal::Signal};
use esp_backtrace as _;
use esp_hal::{
    timer::timg::TimerGroup,
    uart::{AtCmdConfig, Config, Uart, UartRx},
    Async,
};
use static_cell::StaticCell;

const AT_CMD: u8 = 0x0D;

#[embassy_executor::task]
async fn reader(mut rx: UartRx<'static, Async>, signal: &'static Signal<NoopRawMutex, usize>) {
    const MAX_BUFFER_SIZE: usize = 1028;

    let mut rbuf: [u8; MAX_BUFFER_SIZE] = [0u8; MAX_BUFFER_SIZE];
    let mut offset = 0;
    loop {
        let r = embedded_io_async::Read::read(&mut rx, &mut rbuf[offset..]).await;
        match r {
            Ok(len) => {
                offset += len;
                esp_println::println!("{}", core::str::from_utf8(&rbuf[..offset]).unwrap());
                offset = 0;
                signal.signal(len);
            }
            Err(e) => esp_println::println!("RX Error: {:?}", e),
        }
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::println!("Init!");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    // Default pins for Uart/Serial communication
    let config = Config {
        baudrate: 9600,
        ..Default::default()
    };

    let mut uart0 = Uart::new_with_config(peripherals.UART2, config, peripherals.GPIO13, peripherals.GPIO14).unwrap().into_async();
    uart0.set_at_cmd(AtCmdConfig::new(None, None, None, AT_CMD, None));

    //let rx = UartRx::new_with_config(peripherals.UART2, config, peripherals.GPIO13).unwrap().into_async();
    
    static SIGNAL: StaticCell<Signal<NoopRawMutex, usize>> = StaticCell::new();
    let signal = &*SIGNAL.init(Signal::new());

    let (rx, _tx) = uart0.split();

    spawner.spawn(reader(rx, &signal)).ok();
    //spawner.spawn(writer(tx, &signal)).ok();
}