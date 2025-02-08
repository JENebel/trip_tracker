use esp_hal::gpio::AnyPin;

pub struct SDCardStorage {
    
}

impl SDCardStorage {
    pub fn new(
        sclk: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        miso: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        mosi: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        cs: esp_hal::peripheral::PeripheralRef<'static, AnyPin>) -> Self {
            
        Self { }
    }
}