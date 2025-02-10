use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use esp_println::println;
use nmea::{sentences::{FixType, GgaData, VtgData, ZdaData}, ParseResult};

use crate::simcom_modem::{SimComModem, MAX_NMEA_LENGTH};

pub type NMEAChannel = Channel<CriticalSectionRawMutex, ([u8; MAX_NMEA_LENGTH], usize), 16>;

// The 2 first bytes of the NMEA sentence is the main system, 
// but they can be separated by only looking at the second byte.
#[repr(u8)]
#[derive(Debug, Clone)]
pub enum MainSystem {
    Unknown,
    GPS,
    GLONASS,
    Galileo,
    Beidou,
    Combined,
}

impl MainSystem {
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            b'P' => MainSystem::GPS,        // GP
            b'L' => MainSystem::GLONASS,    // GL
            b'A' => MainSystem::Galileo,    // GA
            b'D' => MainSystem::Beidou,     // BD
            b'N' => MainSystem::Combined,   // GN
            _ => MainSystem::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GNSSState {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f32,
    pub timestamp: i64,
    pub speed_knots: f32,
    pub course: f32,
    pub fix_type: FixType,
    pub satellites: u32,
    pub main_system: MainSystem,

    has_vtg: bool,
    is_complete: bool,
}

impl GNSSState {
    fn new_from_gga(gga_data: GgaData, main_system: MainSystem) -> Result<Self, ()> {
        let fix_type = gga_data.fix_type.ok_or(())?;
        let latitude = gga_data.latitude.ok_or(())?;
        let longitude = gga_data.longitude.ok_or(())?;
        let altitude = gga_data.altitude.ok_or(())?;
        let geoid_separation = gga_data.geoid_separation.ok_or(())?;
        let satellites = gga_data.fix_satellites.ok_or(())?;

        Ok(Self {
            latitude,
            longitude,
            altitude: altitude - geoid_separation,
            timestamp: 0,
            speed_knots: 0.0,
            course: 0.0,
            fix_type,
            satellites,
            main_system,

            has_vtg: false,
            is_complete: false,
        })
    }

    fn apply_vtg(mut self, vtg_data: VtgData) -> Result<Self, ()> {
        self.speed_knots = vtg_data.speed_over_ground.ok_or(())?;
        self.course = vtg_data.true_course.ok_or(())?;
        self.has_vtg = true;
        Ok(self)
    }

    fn complete_with_zda(mut self, zda_data: ZdaData) -> Result<Self, ()> {
        self.timestamp = zda_data.utc_date_time().map(|t| t.and_utc().timestamp()).ok_or(())?;
        self.is_complete = self.has_vtg;
        Ok(self)
    }
}

#[embassy_executor::task]
pub async fn gnss_monitor() {
    let channel = SimComModem::get_nmea_channel();

    println!("GNSS monitor started");
 
    let mut state = None;

    loop {
        let (sentence_bytes, length) = channel.receive().await;
        let sentence_bytes = &sentence_bytes[..length];
        match nmea::parse_bytes(&sentence_bytes) {
            Ok(sentence) => match sentence {
                ParseResult::GGA(gga_data) => {
                    let main_system = MainSystem::from_byte(sentence_bytes[2]);
                    state = GNSSState::new_from_gga(gga_data, main_system).ok();

                    if state.is_none() {
                        println!("Failed to create GNSS state");
                    }
                },
                ParseResult::VTG(vtg_data) => {
                    if let Some(old_state) = state.take() {
                        state = old_state.apply_vtg(vtg_data).ok();

                        if state.is_none() {
                            println!("Failed to apply VTG data");
                        }
                    }
                },
                ParseResult::ZDA(zda_data) => {
                    if let Some(old_state) = state.take() {
                        if let Ok(completed) = old_state.complete_with_zda(zda_data) {
                            if completed.is_complete {
                        // println!("{:?}", completed);
                                continue;
                            }
                        }
                    }

                    println!("Failed to complete GNSS state");
                },
                _ => println!("Unknown sentence"),
            },
            Err(err) => println!("Failed to parse sentence {:?} {}", core::str::from_utf8(&sentence_bytes).unwrap(), err),
        }
    }
}