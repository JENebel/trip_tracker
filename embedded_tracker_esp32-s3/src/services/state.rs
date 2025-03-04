use alloc::sync::Arc;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use esp_hal::{analog::adc::{Adc, AdcConfig, AdcPin, Attenuation}, gpio::GpioPin, peripheral::Peripheral, peripherals::ADC1};

use crate::{info, Configuration, Service};
use alloc::boxed::Box;

#[derive(Debug)]
pub struct StateService {
    battery_level: Arc<Mutex<CriticalSectionRawMutex, Option<u8>>>,
}

#[async_trait::async_trait]
impl Service for StateService {
    async fn start(&mut self) {
        
    }

    async fn stop(&mut self) {
        
    }
}

impl StateService {
    pub fn init(
        spawner: &Spawner, 
        battery_adc: impl Peripheral<P = ADC1> + 'static, 
        battery_pin: GpioPin<4> 
    ) -> Self {
        
        let mut adc1_config = AdcConfig::new();
        let mut pin = adc1_config.enable_pin(
            battery_pin,
            Attenuation::Attenuation11dB,
        );
        
        let mut adc1 = Adc::new(battery_adc, adc1_config);
        adc1.read_blocking(&mut pin);

        spawner.must_spawn(device_monitor(adc1, pin));
        Self {
            battery_level: Arc::new(Mutex::new(None)),
        }
    }
}

#[embassy_executor::task]
async fn device_monitor(mut battery_adc: Adc<'static, ADC1>, mut pin: AdcPin<GpioPin<4>, ADC1>) {
    loop {
        // Update battery level

        let v = battery_adc.read_blocking(&mut pin) * 2;

        // enforce bounds, 0-100
       // y = y.max(0.0);
    
    //    y = y.min(100.0);

        info!("Battery voltage: {} mV", v);

        // Update solar level

        embassy_time::Timer::after_secs(60).await;
    }
}

fn battery_percentage(voltage_mv: u32) -> u8 {
    let v_min = 3000; // 3.0V = 0%
    let v_max = 4200; // 4.2V = 100%

    if voltage_mv <= v_min {
        return 0;
    } else if voltage_mv >= v_max {
        return 100;
    }

    let percentage = ((voltage_mv - v_min) as f32 / (v_max - v_min) as f32) * 100.0;
    percentage as u8
}