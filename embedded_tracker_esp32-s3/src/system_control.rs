use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;
use esp_hal::{gpio::{AnyPin, Input, Pull}, peripherals::LPWR, rtc_cntl::{sleep::{self}, Rtc}};
use heapless::Vec;

use alloc::sync::Arc;

use crate::{info, ExclusiveService, Service};

pub struct SystemControl {
    services: Vec<Arc<Mutex<CriticalSectionRawMutex, dyn Service>>, 12>,
    wakeup_pin: Input<'static>,
    sleep_pin: Input<'static>,
    lpwr: LPWR,
}

impl SystemControl {
    pub fn new(
        sleep_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        wake_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        lpwr: LPWR,
    ) -> Self {
        Self {
            services: Vec::new(),
            sleep_pin: Input::new(sleep_pin, Pull::Down),
            wakeup_pin: Input::new(wake_pin, Pull::Down),
            lpwr,
        }
    }

    pub async fn register_service<S: Service + Sized + 'static>(&mut self, service: S) -> ExclusiveService<S> {
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
            let mut service = service.lock().await;
            info!("Stopping {:?}", &service);
            service.stop().await;
        }
    }

    pub fn is_sleep_pin_low(&self) -> bool {
        self.sleep_pin.is_low()
    }

    pub async fn go_to_sleep(mut self) {
        info!("Going to sleep");
        self.stop_services().await;

        let waker = sleep::Ext0WakeupSource::new(self.wakeup_pin, sleep::WakeupLevel::High);
        let mut rtc = Rtc::new(self.lpwr);
        
        rtc.sleep_deep(&[&waker]);
    }

    pub async fn detect_sleep(self) {
        let mut low_count = 0;
        loop {
            Timer::after_secs(1).await;

            // Chek if sleep pin is low
            if self.sleep_pin.is_low() {
                info!("Sleep pin is low");
                low_count += 1;
                if low_count >= 3 {
                    break;
                }
            } else {
                low_count = 0;
            }
        }
        self.go_to_sleep().await;
    }
}