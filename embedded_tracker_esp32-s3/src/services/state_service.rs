use core::fmt::{self, Debug};

use alloc::sync::Arc;
use chrono::{TimeDelta, Utc};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal, watch::Watch};
use embassy_time::{Duration, Instant, Timer, WithTimeout};
use esp_hal::{analog::adc::{Adc, AdcCalBasic, AdcConfig, AdcPin, Attenuation}, gpio::{AnyPin, GpioPin, Input, Output}, peripheral::Peripheral, peripherals::ADC1, prelude::nb};

use crate::{debug, info, ActorTerminator, Service};
use alloc::boxed::Box;

pub static CURRENT_TIME: Watch<CriticalSectionRawMutex, (chrono::DateTime<Utc>, Instant), 5> = Watch::new();

pub fn get_current_time() -> Option<chrono::DateTime<Utc>> {
    let current_time = CURRENT_TIME.dyn_anon_receiver().try_get();
    if let Some((time, instant)) = current_time {
        let offset = instant.elapsed().as_micros() as i64;
        let actual_time = time.checked_add_signed(TimeDelta::microseconds(offset)).unwrap();
        Some(actual_time)
    } else {
        None
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum BatteryStatus {
    Unknown,
    ChargingUSB,
    Charging(u8),
    Discharging(u8),
}

#[derive(Debug)]
pub enum SignalStrength {
    Good, // 20-30
    Ok,   // 10-20
    Bad,  // 0-10
    None  // 99
}

impl SignalStrength {
    pub fn from_rssi(rssi: u8) -> Self {
        if rssi <= 10 {
            Self::Bad
        } else if rssi <= 20 {
            Self::Ok
        } else if rssi <= 30 {
            Self::Good
        } else {
            Self::None
        }
    }
}

#[derive(Debug)]
pub enum BitErrorRate {
    Good, // < 0.01%
    Ok,   // < 4%
    Bad,  // > 2%
    None  // No signal
}

impl BitErrorRate {
    pub fn from_ber(ber: u8) -> Self {
        if ber == 0 {
            Self::Good
        } else if ber <= 4 {
            Self::Ok
        } else if ber <= 7 {
            Self::Bad
        } else {
            Self::None
        }
    }
}

struct DeviceState {
    battery_status: BatteryStatus,
    is_net_connected: Option<bool>,
    has_gnss_fix: bool,
    signal_strength: SignalStrength,
    signal_error_rate: BitErrorRate,
}

pub struct StateService {
    device_state: Arc<Mutex<CriticalSectionRawMutex, DeviceState>>,
    update_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,
    upload_enabled: Input<'static, AnyPin>,
    terminator: ActorTerminator,
}

impl Debug for StateService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "State Service")
    }
}

#[async_trait::async_trait]
impl Service for StateService {
    async fn stop(&mut self) {
        self.terminator.terminate().await;
    }
}

impl StateService {
    pub fn start(
        spawner: &Spawner, 
        power_adc: impl Peripheral<P = ADC1> + 'static, 
        battery_pin: GpioPin<4>,
        solar_pin: GpioPin<5>,

        upload_enabled: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,

        power_led_red: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        power_led_green: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        power_led_blue: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        gnss_led_red: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        gnss_led_green: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        network_led_red: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        network_led_green: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
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

        let terminator = ActorTerminator::new();

        let device_state = Arc::new(Mutex::new(DeviceState {
            battery_status: BatteryStatus::Unknown,
            is_net_connected: None,
            has_gnss_fix: false,
            signal_strength: SignalStrength::None,
            signal_error_rate: BitErrorRate::None,
        }));

        let update_signal = Arc::new(Signal::new());

        spawner.must_spawn(power_monitor(adc, pin_b, pin_s, device_state.clone(), terminator.clone()));
        spawner.must_spawn(state_output(device_state.clone(), update_signal.clone(), power_led_red, power_led_green, power_led_blue, gnss_led_red, gnss_led_green, network_led_red, network_led_green));

        update_signal.signal(true);

        Self {
            device_state,
            update_signal,
            upload_enabled: Input::new(upload_enabled, esp_hal::gpio::Pull::Down),
            terminator,
        }
    }

    pub fn is_upload_enabled(&self) -> bool {
        self.upload_enabled.is_high()
    }

    pub async fn set_signal_quality(&self, rssi: u8, ber: u8) {
        let mut state = self.device_state.lock().await;
        state.signal_strength = SignalStrength::from_rssi(rssi);
        state.signal_error_rate = BitErrorRate::from_ber(ber);
        debug!("Signal strength: {:?}, error rate: {:?}", state.signal_strength, state.signal_error_rate);
        self.update_signal.signal(true);
    }

    pub async fn set_upload_state(&self, is_net_connected: Option<bool>) {
        let mut state = self.device_state.lock().await;
        state.is_net_connected = is_net_connected;
        self.update_signal.signal(true);
    }

    pub async fn set_gnss_state(&self, has_gnss_fix: bool) {
        let mut state = self.device_state.lock().await;
        state.has_gnss_fix = has_gnss_fix;
        self.update_signal.signal(true);
    }
}

// Handle LEDs
#[embassy_executor::task]
async fn state_output(
    device_state: Arc<Mutex<CriticalSectionRawMutex, DeviceState>>,
    update_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,

    power_led_red: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
    power_led_green: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
    power_led_blue: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
    gnss_led_red: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
    gnss_led_green: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
    network_led_red: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
    network_led_green: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
) {
    let mut power_led_red = Output::new(power_led_red, esp_hal::gpio::Level::Low);
    let mut power_led_green = Output::new(power_led_green, esp_hal::gpio::Level::Low);
    let mut power_led_blue = Output::new(power_led_blue, esp_hal::gpio::Level::Low);
    
    let mut gnss_led_red = Output::new(gnss_led_red, esp_hal::gpio::Level::High);
    let mut gnss_led_green = Output::new(gnss_led_green, esp_hal::gpio::Level::Low);

    let mut network_led_red = Output::new(network_led_red, esp_hal::gpio::Level::High);
    let mut network_led_green = Output::new(network_led_green, esp_hal::gpio::Level::Low);

    network_led_green.set_drive_strength(esp_hal::gpio::DriveStrength::I10mA);

    update_signal.signal(true);

    loop {
        let _ = update_signal.wait().with_timeout(Duration::from_secs(2)).await;
        update_signal.reset();

        let state = device_state.lock().await;

        // Update LEDs
        // On/Off based on dip switch 1

        // LED pins:
        match state.battery_status {
            BatteryStatus::Unknown => {
                power_led_blue.set_low();
                power_led_green.set_low();
                power_led_red.set_low();
            },
            BatteryStatus::ChargingUSB => {
                power_led_blue.set_high();
                power_led_green.set_low();
                power_led_red.set_low();
            },
            BatteryStatus::Charging(lvl) | BatteryStatus::Discharging(lvl) => {
                match lvl {
                    0..=33 => {
                        power_led_blue.set_low();
                        power_led_green.set_low();
                        power_led_red.set_high();
                    },
                    34..=66 => {
                        power_led_blue.set_low();
                        power_led_green.set_high();
                        power_led_red.set_high();
                    },
                    67.. => {
                        power_led_blue.set_low();
                        power_led_green.set_high();
                        power_led_red.set_low();
                    },
                }
            },
        }

        // GNSS Red
        // GNSS Green
        if state.has_gnss_fix {
            gnss_led_red.set_low();
            gnss_led_green.set_high();
        } else {
            gnss_led_red.set_high();
            gnss_led_green.set_low();
        }

        // Network Red
        // Network Green
        if let Some(is_net_connected) = state.is_net_connected {
            if is_net_connected {
                network_led_red.set_low();
                network_led_green.set_high();
            } else {
                network_led_red.set_high();
                network_led_green.set_low();
            }
        } else {
            network_led_red.set_low();
            network_led_green.set_low();
        }
    }
}

#[embassy_executor::task]
async fn power_monitor(
    mut adc: Adc<'static, ADC1>, 
    mut pin_b: AdcPin<GpioPin<4>, ADC1, AdcCalBasic<ADC1>>,
    mut pin_s: AdcPin<GpioPin<5>, ADC1, AdcCalBasic<ADC1>>,
    device_state: Arc<Mutex<CriticalSectionRawMutex, DeviceState>>,
    terminator: ActorTerminator,
) {
    let mut previous_battery_state = BatteryStatus::Unknown;
    loop {
        if terminator.is_terminating() {
            terminator.terminated();
            break;
        }

        // Update battery level

        let v_b = nb::block!(adc.read_oneshot(&mut pin_b)).unwrap() * 2;
        let v_s = nb::block!(adc.read_oneshot(&mut pin_s)).unwrap() * 2;

        let battery_state = if v_b < 500 {
            // When usb is connected, pin4 is pulled low
            BatteryStatus::ChargingUSB
        } else {
            let battery_percentage = battery_percentage(v_b);

            // If solar voltage is less than 500mV, then the battery is discharging
            if v_s < 500 {
                BatteryStatus::Discharging(battery_percentage)
            } else {
                BatteryStatus::Charging(battery_percentage)
            }
        };

        info!("Battery state: {:?}", battery_state);

        if battery_state != previous_battery_state {
            device_state.lock().await.battery_status = battery_state.clone();
            previous_battery_state = battery_state;
        }

        // Update solar level
        for _ in 0..60 {
            if terminator.is_terminating() {
                break;
            }
            Timer::after_secs(2).await;
        }
    }
}

fn battery_percentage(voltage_mv: u16) -> u8 {
    let v_min = 3500; // 3.5V = 0%
    let v_max = 4200; // 4.2V = 100%

    if voltage_mv <= v_min {
        return 0;
    } else if voltage_mv >= v_max {
        return 100;
    }

    let percentage = ((voltage_mv - v_min) as f32 / (v_max - v_min) as f32) * 100.0;
    percentage as u8
}

#[macro_export]
macro_rules! fatal_error {
    ($($arg:tt)*) => {{
        $crate::error!($($arg)*);
        
        panic!($($arg)*);
    }}
}