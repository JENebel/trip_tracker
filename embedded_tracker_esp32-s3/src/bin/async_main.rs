#![no_std]
#![no_main]
#![feature(slice_split_once)]
#![feature(impl_trait_in_assoc_type)]

use core::{mem::{forget, MaybeUninit}, panic::PanicInfo};

use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embedded_tracker_esp32_s3::{info, log::Logger, sys_info, ExclusiveService, GNSSService, ModemService, StateService, StorageService, SystemControl, UploadService};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock, cpu_control::{CpuControl, Stack}, delay::Delay, gpio::{AnyPin, Input, Level, Output, Pull}, peripheral::Peripheral, peripherals, reset::{self}, sha::Sha, spi::AnySpi, timer::{timg::TimerGroup, AnyTimer}, uart::AnyUart
};

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

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    embedded_tracker_esp32_s3::error!("Paniced at {:?}", info.location());
    embedded_tracker_esp32_s3::error!("Panic: {:?}", info.message());

    let peripherals = unsafe {
        peripherals::Peripherals::steal()
    };

    // Turn on error LED
    Output::new(peripherals.GPIO16, Level::High);

    // Turn off status led
    Output::new(peripherals.GPIO15, Level::Low);

    // Turn off all other LEDs
    Output::new(peripherals.GPIO2, Level::Low);
    Output::new(peripherals.GPIO42, Level::Low);
    Output::new(peripherals.GPIO41, Level::Low);
    Output::new(peripherals.GPIO40, Level::Low);
    Output::new(peripherals.GPIO39, Level::Low);
    Output::new(peripherals.GPIO38, Level::Low);
    Output::new(peripherals.GPIO48, Level::Low);

    // Activate Buzzer
    let buzzer = peripherals.GPIO8;
    let mut buzzer = Output::new(buzzer, Level::High);
    Delay::new().delay_millis(1000);
    buzzer.set_low();
    Delay::new().delay_millis(4000);
    
    let reset_btn_input = Input::new(peripherals.GPIO7, Pull::Down);

    // Wait for reset pin to be high, and then reset
    loop {
        if reset_btn_input.is_high() {
            reset::software_reset();
        }
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // Initialize heap
    init_heap();

    let mut config = esp_hal::Config::default();
    config.cpu_clock = CpuClock::max();
    let peripherals = esp_hal::init(config);

    let wake_pin = AnyPin::from(peripherals.GPIO6).into_ref();
    let led = peripherals.GPIO15;
    let status_led_pin = AnyPin::from(led).into_ref();
    let mut system = SystemControl::new(wake_pin, status_led_pin, peripherals.LPWR);
    if system.is_sleep_pin_low() {
        info!("Sleep pin is low, entering deep sleep");
        system.go_to_sleep().await;
        unreachable!();
    }

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
    let storage = StorageService::start(sd_spi, sclk, miso, mosi, cs);
    let storage_service = system.register_service(storage).await;
    Logger::start(&spawner, storage_service.clone());

    // Initialize state service
    info!("Initializing state service...");
    let battery_adc = peripherals.ADC1;
    let battery_pin = peripherals.GPIO4;
    let solar_pin = peripherals.GPIO5;

    let upload_enabled = AnyPin::from(peripherals.GPIO1).into_ref();
    
    let gnss_led_red = AnyPin::from(peripherals.GPIO2).into_ref();
    let gnss_led_green = AnyPin::from(peripherals.GPIO42).into_ref();

    let network_led_red = AnyPin::from(peripherals.GPIO41).into_ref();
    let network_led_green = AnyPin::from(peripherals.GPIO40).into_ref();

    let power_led_red = AnyPin::from(peripherals.GPIO39).into_ref();
    let power_led_green = AnyPin::from(peripherals.GPIO38).into_ref();
    let power_led_blue = AnyPin::from(peripherals.GPIO48).into_ref();
    
    let state_service = StateService::start(&spawner, battery_adc, battery_pin, solar_pin, upload_enabled, power_led_red, power_led_green, power_led_blue, gnss_led_red, gnss_led_green, network_led_red, network_led_green);
    let state_service = system.register_service(state_service).await;

    // Initialize modem service
    info!("Initializing modem service...");
    let uart = AnyUart::from(peripherals.UART1).into_ref();
    let rx_pin = AnyPin::from(peripherals.GPIO10).into_ref();
    let tx_pin = AnyPin::from(peripherals.GPIO11).into_ref();
    let modem_reset_pin = AnyPin::from(peripherals.GPIO17).into_ref();
    let pwrkey_pin = AnyPin::from(peripherals.GPIO18).into_ref();
    let modem = ModemService::initialize(&spawner, uart, rx_pin, tx_pin, modem_reset_pin, pwrkey_pin).await;
    let modem_service = system.register_service(modem).await;

    // Initialize upload service, and start on another core
    info!("Initializing upload service...");
    let upload = init_upload_service(
        CpuControl::new(peripherals.CPU_CTRL), 
        Sha::new(peripherals.SHA), 
        modem_service.clone(),
        storage_service.clone(),
        state_service.clone(),
    ).await;
    let upload_service = system.register_service(upload).await;

    // Initialize GNSS service
    info!("Initializing GNSS service...");
    let gnss = GNSSService::start(&spawner, storage_service.clone(), modem_service.clone(), upload_service.clone(), state_service.clone()).await;
    let _gnss_service = system.register_service(gnss).await;

    // Start services

    sys_info!("All running!");

    // Detect sleep
    system.detect_sleep().await;
}

static UPLOAD_SERVICE_LOCK: Signal<CriticalSectionRawMutex, UploadService> = Signal::new();

async fn init_upload_service(
    mut cpu_control: CpuControl<'static>, 
    sha: Sha<'static>,
    modem_service: ExclusiveService<ModemService>, 
    storage_service: ExclusiveService<StorageService>,
    state_service: ExclusiveService<StateService>,
) -> UploadService {
    let _core1_guard = cpu_control.start_app_core(unsafe { &mut *core::ptr::addr_of_mut!(APP_CORE_STACK) }, move || {
        static EXECUTOR: StaticCell<Executor> = StaticCell::new();
        let executor = EXECUTOR.init(Executor::new());
        executor.run(|spawner| {
            spawner.spawn(core1_task(spawner, sha, modem_service, storage_service, state_service)).unwrap();
        });
    }).unwrap();
    forget(_core1_guard);

    UPLOAD_SERVICE_LOCK.wait().await
}

#[embassy_executor::task]
async fn core1_task(
    spawner: Spawner, 
    sha: Sha<'static>, 
    modem_service: ExclusiveService<ModemService>, 
    storage_service: ExclusiveService<StorageService>,
    state_service: ExclusiveService<StateService>,
) {
    let upload_service = UploadService::start(&spawner, sha, modem_service, storage_service, state_service).await;
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