use alloc::sync::Arc;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use esp_hal::{analog::adc::{Adc, AdcCalBasic, AdcCalScheme, AdcCalSource, AdcChannel, AdcConfig, AdcPin, Attenuation, Resolution}, gpio::GpioPin, peripheral::Peripheral, peripherals::{ADC1, ADC2}, prelude::nb};

use crate::{info, Service};
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
        power_adc: impl Peripheral<P = ADC1> + 'static, 
        battery_pin: GpioPin<4>,
        solar_pin: GpioPin<5>,
    ) -> Self {
        let mut adc_config = AdcConfig::new();

        let pin_b = adc_config.enable_pin_with_cal::<GpioPin<4>, AdcCalBasic<ADC1>>(
            battery_pin, 
            Attenuation::Attenuation11dB
        );

        let pin_s = adc_config.enable_pin_with_cal::<GpioPin<5>, AdcCalBasic<ADC1>>(
            solar_pin, 
            Attenuation::Attenuation11dB
        );

        let adc = Adc::new(power_adc, adc_config);

        spawner.must_spawn(device_monitor(adc, pin_b, pin_s));

        Self {
            battery_level: Arc::new(Mutex::new(None)),
        }
    }
}

#[embassy_executor::task]
async fn device_monitor(
    mut adc: Adc<'static, ADC1>, 
    mut pin_b: AdcPin<GpioPin<4>, ADC1, AdcCalBasic<ADC1>>,
    mut pin_s: AdcPin<GpioPin<5>, ADC1, AdcCalBasic<ADC1>>,
) {
    loop {
        // Update battery level

        let v_b = nb::block!(adc.read_oneshot(&mut pin_b)).unwrap();
        let v_s = nb::block!(adc.read_oneshot(&mut pin_s)).unwrap();

        // enforce bounds, 0-100
       // y = y.max(0.0);
    
    //    y = y.min(100.0);

        info!("Battery voltage: {} mV, estimated {}%", v_b, battery_percentage(v_b));
        info!("Solar voltage: {} mV", v_s);

        // Update solar level

        embassy_time::Timer::after_secs(5/*60 * 5*/).await;
    }
}

fn battery_percentage(voltage_mv: u16) -> u8 {
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