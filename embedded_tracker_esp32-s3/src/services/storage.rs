use core::fmt;

use chrono::{DateTime, Datelike, Timelike, Utc};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{Mode, RawDirectory, RawFile, SdCard, TimeSource, Timestamp, VolumeManager};
use esp_hal::{delay::Delay, gpio::{AnyPin, Level, Output}, prelude::*, spi::{master::{Config, Spi}, AnySpi}, Blocking};
use esp_println::println;
use trip_tracker_lib::track_point::TrackPoint;
use alloc::fmt::Debug;

use crate::{configuration::Configuration, debug, error, Service};
use alloc::boxed::Box;

const MAX_DIRS: usize = 128;
const MAX_FILES: usize = 128;
const MAX_VOLUMES: usize = 1;

type BlockingSPISDCard = SdCard<ExclusiveDevice<Spi<'static, Blocking>, Output<'static>, Delay>, Delay>;

pub struct StorageService {
    session_id: u32,
    configuration: Configuration,
    start_time: Option<DateTime<Utc>>,

    volume_mgr: VolumeManager<BlockingSPISDCard, Timesource, MAX_DIRS, MAX_FILES, MAX_VOLUMES>,

    root_dir: RawDirectory,
    state_file: RawFile,
    sys_log_file: RawFile,

    sessions_dir: RawDirectory,
    session_file: RawFile,
    session_log_file: RawFile,
}

#[async_trait::async_trait]
impl Service for StorageService {
    async fn start(&mut self) {
        
    }

    async fn stop(&mut self) {
        
    }
}

impl Debug for StorageService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StorageService {{ session_id: {}, configuration: {:?}, start_time: {:?} }}", self.session_id, self.configuration, self.start_time)
    }
}

impl StorageService {
    pub fn set_start_time(&mut self, time: DateTime<Utc>) {
        self.start_time = Some(time);

        debug!("Set start time: {}", time);
        
        let bytes = time.timestamp().to_be_bytes();
        self.volume_mgr.write(self.session_file, &bytes).unwrap();
        debug!("{:?}", time.timestamp());
        debug!("{:?}", time.timestamp().to_be_bytes());
        self.volume_mgr.flush_file(self.session_file).unwrap();
    }

    pub fn append_track_point(&mut self, track_point: TrackPoint) {
        let start_time = self.start_time.unwrap();
        let bytes = track_point.to_bytes(start_time);
        self.volume_mgr.write(self.session_file, &bytes).unwrap();
        self.volume_mgr.flush_file(self.session_file).unwrap();
    }

    pub fn append_to_sys_log(&mut self, bytes: &[u8]) {
        self.volume_mgr.write(self.sys_log_file, bytes).unwrap();
        self.volume_mgr.flush_file(self.sys_log_file).unwrap();
    }

    pub fn append_to_session_log(&mut self, bytes: &[u8]) {
        self.volume_mgr.write(self.session_log_file, bytes).unwrap();
        self.volume_mgr.flush_file(self.session_log_file).unwrap();
    }

    pub fn initialize(
        spi: esp_hal::peripheral::PeripheralRef<'static, AnySpi>,
        sclk: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        miso: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        mosi: esp_hal::peripheral::PeripheralRef<'static, AnyPin>,
        cs: esp_hal::peripheral::PeripheralRef<'static, AnyPin>
    ) -> Self {
        let spi_config = Config {
            frequency: 40_000.kHz(),
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
    
        let mut volume_mgr = VolumeManager::new_with_limits(sdcard, Timesource::default(), 0);

        let volume = volume_mgr.open_raw_volume(embedded_sdmmc::VolumeIdx(0)).unwrap();
        let root_dir = volume_mgr.open_root_dir(volume).unwrap();

        if volume_mgr.find_directory_entry(root_dir, "SESSIONS").is_err() {
            volume_mgr.make_dir_in_dir(root_dir, "SESSIONS").unwrap();
        }

        let configuration = {
            // Check if file too large
            let Ok(config_file) = volume_mgr.open_file_in_dir(root_dir, "CONFIG.CFG", Mode::ReadOnly) else {
                error!("No config file found"); // Maybe write a default?
                panic!();
            };

            let mut config_file = config_file.to_file(&mut volume_mgr);

            if config_file.length() > 512 {
                error!("Config file too large");
                panic!();
            }

            let buffer = &mut [0u8; 512];
            let bytes = config_file.read(buffer).unwrap();
            let config_str = core::str::from_utf8(&buffer[..bytes]).unwrap();
            let cfg = Configuration::parse(config_str);
            debug!("Config: {:?}", cfg);

            cfg
        };

        let Ok(state_file) = volume_mgr.open_file_in_dir(root_dir, "STATE.CSV", Mode::ReadWriteCreateOrAppend) else {
            panic!("No STATE.CSV file found");
        };

        let Ok(sys_log_file) = volume_mgr.open_file_in_dir(root_dir, "SYSTEM.LOG", Mode::ReadWriteCreateOrAppend) else {
            panic!("No SYSTEM.LOG file found");
        };
        
        // Count session dirs to determine current session ID
        let mut session_id = 0;
        let sessions_dir = volume_mgr.open_dir(root_dir, "SESSIONS").unwrap();
        volume_mgr.iterate_dir(sessions_dir, |e| {
            if e.attributes.is_directory() {
                session_id += 1;
            }
        }).unwrap();
        println!("Session ID: {}", session_id);

        let mut buffer = itoa::Buffer::new();
        let session_num_str = buffer.format(session_id);

        volume_mgr.make_dir_in_dir(sessions_dir, session_num_str).unwrap();

        let session_dir = volume_mgr.open_dir(sessions_dir, session_num_str).unwrap();
        let session_file = volume_mgr.open_file_in_dir(session_dir, "SESSION.TSF", Mode::ReadWriteCreateOrAppend).unwrap();
        let session_log_file = volume_mgr.open_file_in_dir(session_dir, "SESSION.LOG", Mode::ReadWriteCreateOrAppend).unwrap();

        Self {
            configuration,
            session_id,
            start_time: None,

            volume_mgr,

            root_dir,
            state_file,
            sys_log_file,

            sessions_dir,
            session_file,
            session_log_file,
        }
    }
}

#[derive(Default)]
pub struct Timesource(DateTime<Utc>);

impl TimeSource for Timesource {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: self.0.years_since(DateTime::from_timestamp(0, 0).unwrap()).unwrap() as u8,
            zero_indexed_month: self.0.month0() as u8,
            zero_indexed_day: self.0.day0() as u8,
            hours: self.0.hour() as u8,
            minutes: self.0.minute() as u8,
            seconds: self.0.second() as u8,
        }
    }
}