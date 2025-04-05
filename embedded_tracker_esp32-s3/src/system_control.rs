use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker, Timer};
use esp_hal::{gpio::{AnyPin, Input, Output, Pull}, peripherals::LPWR, rtc_cntl::{sleep::{self}, Rtc}};
use heapless::Vec;

use alloc::sync::Arc;

use crate::{info, ExclusiveService, Service};

pub struct SystemControl {
    services: Vec<Arc<Mutex<CriticalSectionRawMutex, dyn Service>>, 12>,
    wake_pin: Input<'static>,
    status_led: Output<'static>,
    lpwr: LPWR,
}

impl SystemControl {
    pub fn new(
        sleep_pin: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        status_led: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        lpwr: LPWR,
    ) -> Self {
        Self {
            services: Vec::new(),
            wake_pin: Input::new(sleep_pin, Pull::Down),
            status_led: Output::new(status_led, esp_hal::gpio::Level::High),
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
        self.wake_pin.is_low()
    }

    pub async fn go_to_sleep(mut self) {
        info!("Going to sleep");
        self.stop_services().await;

        let waker = sleep::Ext0WakeupSource::new(self.wake_pin, sleep::WakeupLevel::High);
        let mut rtc = Rtc::new(self.lpwr);
        
        rtc.sleep_deep(&[&waker]);
    }

    pub async fn detect_sleep(mut self) {
        let mut low_count = 0;

        let mut ticker = Ticker::every(Duration::from_secs(1));

        loop {
            ticker.next().await;

            // Chek if sleep pin is low
            if self.wake_pin.is_low() {
                info!("Sleep pin is low");
                low_count += 1;
                if low_count >= 4 {
                    break;
                }
                // Blink LED
                for _ in 0..2 {
                    self.status_led.set_low();
                    Timer::after(Duration::from_millis(250)).await;
                    self.status_led.set_high();
                    Timer::after(Duration::from_millis(250)).await;
                }
            } else {
                low_count = 0;
            }
        }
        self.status_led.set_low();
        self.go_to_sleep().await;
    }
}