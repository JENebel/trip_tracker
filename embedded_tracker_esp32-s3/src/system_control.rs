use core::{cell::RefCell, time::{self, Duration}};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use esp_hal::{delay::Delay, gpio::{self, AnyPin, Event, Input, Io, Level, Output, Pull}, interrupt, macros::{handler, ram}, peripherals::{IO_MUX, LPWR, SW_INTERRUPT, SYSTEM}, rtc_cntl::{sleep::{self, GpioWakeupSource, RtcSleepConfig, TimerWakeupSource, WakeSource, WakeTriggers}, Rtc}, system, InterruptConfigurable};
use esp_println::println;
use heapless::Vec;

use alloc::sync::Arc;

use crate::{debug, info, ExclusiveService, ModemService, Service};

static SLEEP_SIGNAL: Signal<CriticalSectionRawMutex, bool> = Signal::new();

pub struct SystemControl {
    services: Vec<Arc<Mutex<CriticalSectionRawMutex, dyn Service>>, 12>,
}

impl SystemControl {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    pub async fn register_and_start_service<S: Service + Sized + 'static>(&mut self, mut service: S) -> ExclusiveService<S> {
        service.start().await;
        let service = Arc::new(Mutex::<CriticalSectionRawMutex, S>::new(service));
        match self.services.push(service.clone()) {
            Ok(_) => {}
            Err(_) => panic!("Failed to register service"),
        }
        ExclusiveService::new(service)
    }

    // Stop in reversed order, to ensure dependencies are stopped first
    pub async fn stop_services(&mut self) {
        for service in self.services.iter().rev() {
            info!("Stopping {:?}", service);
            service.lock().await.stop().await;
        }
    }

    pub async fn detect_sleep(
        &mut self, 
        wake_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        sleep_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        lpwr: LPWR,
    ) {
        let sleep_pin = Input::new(sleep_pin, Pull::Down);
        
        let mut low_count = 0;
        loop {
            Timer::after_secs(1).await;

            // Chek if sleep pin is low
            if sleep_pin.is_low() {
                low_count += 1;
                if low_count >= 3 {
                    break;
                }
            } else {
                low_count = 0;
            }
        }

        info!("Shutting down");
        self.stop_services().await;

        let pin = Input::new(wake_pin, Pull::Down);
        let waker = sleep::Ext0WakeupSource::new(pin, sleep::WakeupLevel::High);
        let mut rtc = Rtc::new(lpwr);
        
        info!("Going to sleep!");
        Timer::after_secs(1).await;

        rtc.sleep_deep(&[&waker]);
    }
}