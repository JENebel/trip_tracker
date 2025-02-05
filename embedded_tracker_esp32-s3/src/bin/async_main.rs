#![no_std]
#![no_main]
#![feature(slice_split_once)]
mod gps;
mod sim7670g;


use core::{mem::forget, ptr::addr_of_mut};

use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    cpu_control::{CpuControl, Stack}, delay::Delay, gpio::{AnyPin, Level, Output}, peripheral::{self, Peripheral}, timer::{timg::TimerGroup, AnyTimer}, uart::{self, AnyUart, AtCmdConfig, Uart}, Cpu
};

use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal_embassy::Executor;
use esp_println::println;
use sim7670g::{Simcom7670, SIM7670G};
use static_cell::StaticCell;

static mut APP_CORE_STACK: Stack<8192> = Stack::new();

#[embassy_executor::task]
async fn core1_task(led_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>) {
    
    let mut led = Output::new(led_pin, Level::Low);

    loop {
        Timer::after_millis(250).await;
        led.toggle();
        Timer::after_millis(1750).await;
        led.toggle();
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize timers for Embassy
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let timer0: AnyTimer = timg0.timer0.into();
    let timer1: AnyTimer = timg1.timer0.into();
    esp_hal_embassy::init([timer0, timer1]);

    let modem_reset_pin = AnyPin::from(peripherals.GPIO17).into_ref();
    let pwrkey_pin = AnyPin::from(peripherals.GPIO18).into_ref();
    reset(modem_reset_pin, pwrkey_pin);

    let uart = AnyUart::from(peripherals.UART1).into_ref();
    let rx_pin = AnyPin::from(peripherals.GPIO10).into_ref();
    let tx_pin = AnyPin::from(peripherals.GPIO11).into_ref();
    Simcom7670::initialize(&spawner, uart, rx_pin, tx_pin).await;

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);

    // **Start AppCpu**
    let led_pin = AnyPin::from(peripherals.GPIO12).into_ref();
    let _core1_guard = cpu_control.start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
        static EXECUTOR: StaticCell<Executor> = StaticCell::new();
        let executor = EXECUTOR.init(Executor::new());
        executor.run(|spawner| {
            spawner.spawn(core1_task(led_pin)).unwrap();
        });
    }).unwrap();
    forget(_core1_guard);

    println!("Enabling GNSS...");
    SIM7670G.lock().await.as_mut().unwrap().enable_gnss().await.unwrap();

    match SIM7670G.lock().await.as_mut().unwrap().interrogate("AT").await {
        Ok(ok) => println!("{}", ok),
        Err(e) => println!("{}", e),
    }

    /*loop {
        let res = SIM7670G.lock().await.as_mut().unwrap().interrogate("AT").await;
        match res {
            Ok(ok) => println!("{}", ok),
            Err(e) => println!("{}", e),
        }
    }*/
}

fn reset(
    modem_reset_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
    pwrkey_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>) {
        
    let delay = Delay::new();

    println!("Resetting modem...");

    /*let mut simcom_reset = Output::new(modem_reset_pin, Level::Low);
    simcom_reset.set_high();
    delay.delay_millis(100);
    simcom_reset.set_low();
    delay.delay_millis(2600);
    simcom_reset.set_high();*/

    println!("PWRKEY pin cycle...");
    let mut simcom_power = Output::new(pwrkey_pin, Level::Low);
    delay.delay_millis(100);
    simcom_power.set_high();
    delay.delay_millis(1000);
    simcom_power.set_low();
    
    println!("Modem reset complete!");
}