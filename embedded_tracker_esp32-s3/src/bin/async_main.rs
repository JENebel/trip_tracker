#![no_std]
#![no_main]
#![feature(slice_split_once)]
#![feature(impl_trait_in_assoc_type)]

use core::{mem::{forget, MaybeUninit}, ptr::addr_of_mut};

use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embedded_tracker_esp32_s3::{info, log::Logger, sys_info, ExclusiveService, GNSSService, ModemService, StateService, StorageService, SystemControl, UploadService};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock, cpu_control::{CpuControl, Stack}, gpio::AnyPin, peripheral::Peripheral, sha::Sha, spi::AnySpi, timer::{timg::TimerGroup, AnyTimer}, uart::AnyUart
};

use embassy_time::Timer;
use esp_hal_embassy::Executor;
use static_cell::StaticCell;

static mut APP_CORE_STACK: Stack<8192> = Stack::new();

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

    // Initialize state service
    info!("Initializing state service...");
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

    // Initialize upload service, and start on another core
    info!("Initializing upload service...");
    let upload = init_upload_service(CpuControl::new(peripherals.CPU_CTRL), Sha::new(peripherals.SHA), modem_service.clone(), storage_service.clone()).await;
    let upload_service = system.register_and_start_service(upload).await;

    // Initialize GNSS service
    info!("Initializing GNSS service...");
    let led_pin = AnyPin::from(peripherals.GPIO12).into_ref();
    let gnss = GNSSService::initialize(&spawner, storage_service.clone(), modem_service.clone(), upload_service.clone(), led_pin).await;
    let gnss_service = system.register_and_start_service(gnss).await;

    // Start services

    sys_info!("All running!");


    // **Start AppCpu**

    

    

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

static UPLOAD_SERVICE_LOCK: Signal<CriticalSectionRawMutex, UploadService> = Signal::new();

async fn init_upload_service(mut cpu_control: CpuControl<'static>, sha: Sha<'static>, modem_service: ExclusiveService<ModemService>, storage_service: ExclusiveService<StorageService>) -> UploadService {
    info!("Initializing upload service...");
    let _core1_guard = cpu_control.start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
        static EXECUTOR: StaticCell<Executor> = StaticCell::new();
        let executor = EXECUTOR.init(Executor::new());
        executor.run(|spawner| {
            spawner.spawn(core1_task(spawner, sha, modem_service, storage_service)).unwrap();
        });
    }).unwrap();
    forget(_core1_guard);

    UPLOAD_SERVICE_LOCK.wait().await
}

#[embassy_executor::task]
async fn core1_task(spawner: Spawner, sha: Sha<'static>, modem_service: ExclusiveService<ModemService>, storage_service: ExclusiveService<StorageService>) {
    let upload_service = UploadService::initialize(&spawner, sha, modem_service, storage_service).await;
    UPLOAD_SERVICE_LOCK.signal(upload_service);
}

/*
let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
let _core1_guard = cpu_control.start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
    static EXECUTOR: StaticCell<Executor> = StaticCell::new();
    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(gnss::gnss_monitor()).unwrap();
    });
}).unwrap();
forget(_core1_guard);
*/