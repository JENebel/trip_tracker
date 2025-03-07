#![no_std]
#![no_main]
#![feature(slice_split_once)]
#![feature(impl_trait_in_assoc_type)]

use core::{hash, mem::MaybeUninit};

use embassy_executor::Spawner;
use embedded_tracker_esp32_s3::{info, log::Logger, sys_info, ExclusiveService, GNSSService, ModemService, StateService, StorageService, SystemControl, UploadService};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    analog::adc::{Adc, AdcConfig, Attenuation}, clock::CpuClock, cpu_control::Stack, gpio::{AnyPin, Level, Output}, hmac::{Hmac, HmacPurpose, KeyId}, peripheral::Peripheral, prelude::nb::block, sha::{Sha, Sha256}, spi::AnySpi, timer::{timg::TimerGroup, AnyTimer}, uart::AnyUart
};

use embassy_time::Timer;
use esp_println::println;

static mut _APP_CORE_STACK: Stack<8192> = Stack::new();

#[embassy_executor::task]
async fn core1_task(led_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>) {
    
    let mut led = Output::new(led_pin, Level::Low);

    loop {
        /*Timer::after_millis(250).await;
        led.toggle();*/
        Timer::after_millis(1750).await;
        //led.toggle();
    }
}

fn init_heap() {
    const HEAP_SIZE: usize = 32 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            HEAP.as_mut_ptr() as *mut u8,
            HEAP_SIZE,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // Initialize heap
    init_heap();

    let mut system = SystemControl::new();

    let mut config = esp_hal::Config::default();
    config.cpu_clock = CpuClock::max();
    let peripherals = esp_hal::init(config);

    /*let led = peripherals.GPIO12;
    let led_pin = AnyPin::from(led).into_ref();
    let led = Output::new(led_pin, Level::Low); */

    // Initialize timers for Embassy
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let timer0: AnyTimer = timg0.timer0.into();
    let timer1: AnyTimer = timg1.timer0.into();
    esp_hal_embassy::init([timer0, timer1]);

    // Initialize SD card service
    info!("Initializing SD card service...");
    let sd_spi = AnySpi::from(peripherals.SPI2).into_ref();
    let sclk = AnyPin::from(peripherals.GPIO21).into_ref();
    let miso = AnyPin::from(peripherals.GPIO47).into_ref();
    let mosi = AnyPin::from(peripherals.GPIO14).into_ref();
    let cs = AnyPin::from(peripherals.GPIO13).into_ref();
    let storage = StorageService::initialize(sd_spi, sclk, miso, mosi, cs);
    let storage_service = system.register_and_start_service(storage).await;
    Logger::start(&spawner, storage_service.clone());





    /*let mut source_data = "HELLO, ESPRESSIF!".as_bytes();
    let mut sha = Sha::new(peripherals.SHA);
    let mut hasher = sha.start::<Sha256>();
    // Short hashes can be created by decreasing the output buffer to the
    // desired length
    let mut output = [0u8; 32];
    while !source_data.is_empty() {
        // All the HW Sha functions are infallible so unwrap is fine to use if
        // you use block!
        source_data = block!(hasher.update(source_data)).unwrap();
    }
    // Finish can be called as many times as desired to get multiple copies of
    // the output.
    block!(hasher.finish(output.as_mut_slice())).unwrap();*/




    // Initialize state service
    let battery_adc = peripherals.ADC1;
    let battery_pin = peripherals.GPIO4;
    let state_service = StateService::init(&spawner, battery_adc, battery_pin);
    let state_service = system.register_and_start_service(state_service).await;

    // Initialize modem service
    info!("Initializing modem service...");
    let uart = AnyUart::from(peripherals.UART1).into_ref();
    let rx_pin = AnyPin::from(peripherals.GPIO10).into_ref();
    let tx_pin = AnyPin::from(peripherals.GPIO11).into_ref();
    let modem_reset_pin = AnyPin::from(peripherals.GPIO17).into_ref();
    let pwrkey_pin = AnyPin::from(peripherals.GPIO18).into_ref();
    let modem = ModemService::initialize(&spawner, uart, rx_pin, tx_pin, modem_reset_pin, pwrkey_pin).await;
    let modem_service = system.register_and_start_service(modem).await;

    // Initialize GNSS service
    info!("Initializing GNSS service...");
    let led_pin = AnyPin::from(peripherals.GPIO12).into_ref();
    let gnss = GNSSService::initialize(&spawner, storage_service.clone(), modem_service.clone(), led_pin).await;
    let gnss_service = system.register_and_start_service(gnss).await;

    // Initialize upload service
    info!("Initializing upload service...");
    let upload = UploadService::initialize(modem_service.clone(), storage_service.clone()).await;
    let upload_service = system.register_and_start_service(upload).await;

    // Start services

    sys_info!("All running!");


    // **Start AppCpu**

    /*let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    let _core1_guard = cpu_control.start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
        static EXECUTOR: StaticCell<Executor> = StaticCell::new();
        let executor = EXECUTOR.init(Executor::new());
        executor.run(|spawner| {
            spawner.spawn(gnss::gnss_monitor()).unwrap();
        });
    }).unwrap();
    forget(_core1_guard);*/

    

    

    //loop {
        /*let res = modem.interrogate_timeout("AT+CREG?", 5000).await.unwrap();
        println!("{}", res);
        Timer::after_millis(1000).await;*/
    //}

    //modem.send("AT+HTTPINIT").await.unwrap();
    /*modem.send("AT+HTTPPARA=\"URL\",http://httpbin.org/ip").await.unwrap();

    modem.send("AT+HTTPACTION=0").await.unwrap(); // Send request
    let response = modem.interrogate("AT+HTTPREAD=0,500").await.unwrap(); // Read response

    println!("Response: {}", response);

    modem.send("AT+HTTPTERM").await.unwrap();*/

    //let mut led = Output::new(led_pin, Level::Low);
    //let mut ticker = Ticker::every(Duration::from_secs(1));
    /*loop {
        let res = SimComModem::aqcuire().await.interrogate_timeout("AT+CCLK?", 800).await;
        match res {
            Ok(ok) => println!("{}", ok),
            Err(e) => println!("{}", e),
        }
        ticker.next().await;
    }*/

    /*let res = modem_service.lock().await.interrogate_urc("AT+CMGL=\"ALL\"", "+CMGL", 5000).await;
    println!("{:?}", res);*/

    //let mut modem = SimComModem::aqcuire().await;
    /* // SMS
    let res = modem.interrogate("AT+CMGF=1").await;
    println!("CMGF: {:?}", res);

    let res = modem.interrogate("AT+CNMI=2,1,0,0,0").await;
    println!("CNMI: {:?}", res);*/

    //setup_network().await;

    //setup_network(modem_service).await;

    Timer::after_secs(10).await;
}