use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_println::println;
use heapless::String;
use nmea::{sentences::FixType, Nmea, ParseResult, SentenceType};

use crate::simcom_modem::SimComModem;

pub type NMEAChannel = Channel<CriticalSectionRawMutex, String<100>, 4>;

// The 2 first bytes of the NMEA sentence is the main system, 
// but they can be separated by only looking at the second byte.
#[repr(u8)]
#[derive(Debug, Clone)]
pub enum MainSystem {
    GPS      = b'P', // GP
    GLONASS  = b'L', // GL
    Galileo  = b'A', // GA
    Beidou   = b'D', // BD 
    COMBINED = b'N', // GN
}

#[derive(Debug, Clone)]
pub struct GNSSState {
    pub latitude: f32,
    pub longitude: f32,
    pub altitude: f32,
    pub timestamp: u32,
    pub speed: f32,
    pub course: f32,
    pub fix_type: FixType,
    pub satellites: u8,
    pub main_system: MainSystem,
}

#[embassy_executor::task]
pub async fn read_nmea() {
    let channel = SimComModem::get_nmea_channel();
    loop {
        let mut state = GNSSState {
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
            timestamp: 0,
            speed: 0.0,
            course: 0.0,
            fix_type: FixType::Invalid,
            satellites: 0,
            main_system: MainSystem::COMBINED,
        };


        let sentence = channel.receive().await;
        if let Ok(sentence) = nmea::parse_str(&sentence) {
            match sentence {
                ParseResult::GGA(gga_data) => {
                    println!("GGA: {:?}", gga_data);
                    let fix_type = gga_data.fix_type;
                },
                ParseResult::VTG(vtg_data) => {
                    println!("VTG: {:?}", vtg_data);
                },
                ParseResult::ZDA(zda_data) => {
                    let timestamp = zda_data.utc_date_time().map(|t| t.and_utc().timestamp());
                    println!("ZDA: {:?}", timestamp);
                },
                _ => (),
            }
        }
    }
}