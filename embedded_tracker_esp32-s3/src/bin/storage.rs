use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{sdcard, SdCard, TimeSource, Timestamp, VolumeManager};
use esp_hal::{delay::Delay, gpio::{AnyPin, Level, Output}, spi::{master::{Config, Spi}, AnySpi, SpiMode}};
use esp_println::println;

pub struct SDCardStorage {
    
}

impl SDCardStorage {
    pub fn new(
        spi: esp_hal::peripheral::PeripheralRef<'static, AnySpi>,
        sclk: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        miso: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        mosi: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        cs: esp_hal::peripheral::PeripheralRef<'static, AnyPin>) -> Self {
            
        let spi_config = Config {
           // frequency: 400.kHz(),
            mode: SpiMode::Mode0,
            ..Config::default()
        };
        let spi = Spi::new_with_config(spi, spi_config)
            .with_sck(sclk)
            .with_miso(miso)
            .with_mosi(mosi);

        let delay = Delay::new();
        let sd_cs = Output::new(cs, Level::High);
        let spi = ExclusiveDevice::new(spi, sd_cs, delay).unwrap();

        let sdcard = SdCard::new(spi, delay);

        let mut volume_mgr = VolumeManager::new(sdcard, DummyTimesource::default());
        
        let sd_size = volume_mgr.device().num_bytes().unwrap();
        println!("card size is {} bytes", sd_size);

        Self { }
    }
}

#[derive(Default)]
pub struct DummyTimesource();

impl TimeSource for DummyTimesource {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 0,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
        }
    }
}