use core::fmt::{self, Debug};

use chrono::{DateTime, Datelike, Timelike, Utc};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{Mode, RawDirectory, RawFile, SdCard, TimeSource, Timestamp, VolumeManager};
use esp_hal::{delay::Delay, gpio::{AnyPin, Level, Output}, peripheral::PeripheralRef, prelude::*, spi::{master::{Config, Spi}, AnySpi}, Blocking};
use esp_println::println;
use trip_tracker_lib::track_point::{TrackPoint, ENCODED_LENGTH};
use alloc::{boxed::Box, format, string::String, vec::Vec};
use alloc::vec;

use crate::{configuration::Configuration, debug, error, Service};

use super::comms::upload_status::UploadStatus;

const MAX_DIRS: usize = 128;
const MAX_FILES: usize = 128;
const MAX_VOLUMES: usize = 1;

type BlockingSPISDCard = SdCard<ExclusiveDevice<Spi<'static, Blocking>, Output<'static>, Delay>, Delay>;

pub struct StorageService {
    volume_mgr: VolumeManager<BlockingSPISDCard, Timesource, MAX_DIRS, MAX_FILES, MAX_VOLUMES>,

    root_dir: RawDirectory,
    config_file: RawFile,
    upload_status_file: RawFile,
    sys_log_file: RawFile,
    sessions_dir: RawDirectory,

    local_session_id: Option<u32>,
    start_time: Option<DateTime<Utc>>,
    session_file: Option<RawFile>,
    session_log_file: Option<RawFile>,
}

#[async_trait::async_trait]
impl Service for StorageService {
    async fn start(&mut self) {
        // Count session dirs to determine current session ID
        let mut local_id = 0;
        self.volume_mgr.iterate_dir(self.sessions_dir, |e| {
            if e.attributes.is_directory() {
                local_id += 1;
            }
        }).unwrap();
        println!("Session ID: {}", local_id);
        self.local_session_id = Some(local_id);

        let session_num_str = format!("{}", local_id);

        self.volume_mgr.make_dir_in_dir(self.sessions_dir, session_num_str.as_str()).unwrap();

        let session_dir = self.volume_mgr.open_dir(self.sessions_dir, session_num_str.as_str()).unwrap();
        self.session_file = Some(self.volume_mgr.open_file_in_dir(session_dir, "SESSION.TSF", Mode::ReadWriteCreateOrAppend).unwrap());
        self.session_log_file = Some(self.volume_mgr.open_file_in_dir(session_dir, "SESSION.LOG", Mode::ReadWriteCreateOrAppend).unwrap());
    }

    async fn stop(&mut self) {
        self.start_time = None;
        self.session_file = None;
        self.session_log_file = None;
    }
}

impl Debug for StorageService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StorageService {{ session_id: {:?}, start_time: {:?} }}", self.local_session_id, self.start_time)
    }
}

impl StorageService {
    pub fn set_start_time(&mut self, time: DateTime<Utc>) {
        self.start_time = Some(time);

        debug!("Set start time: {}", time);
        
        let bytes = time.timestamp().to_be_bytes();
        self.volume_mgr.write(self.session_file.unwrap(), &bytes).unwrap();
        self.volume_mgr.flush_file(self.session_file.unwrap()).unwrap();
    }

    pub fn append_track_point(&mut self, track_point: TrackPoint) {
        let start_time = self.start_time.unwrap();
        let bytes = track_point.to_bytes(start_time);
        self.volume_mgr.write(self.session_file.unwrap(), &bytes).unwrap();
        self.volume_mgr.flush_file(self.session_file.unwrap()).unwrap();
    }

    pub fn append_to_sys_log(&mut self, bytes: &[u8]) {
        self.volume_mgr.write(self.sys_log_file, bytes).unwrap();
        self.volume_mgr.flush_file(self.sys_log_file).unwrap();
    }

    pub fn append_to_session_log(&mut self, bytes: &[u8]) {
        self.volume_mgr.write(self.session_log_file.unwrap(), bytes).unwrap();
        self.volume_mgr.flush_file(self.session_log_file.unwrap()).unwrap();
    }

    pub fn get_session_track_point_count(&mut self, local_id: u32) -> usize {
        let file = if self.local_session_id == Some(local_id) {
            self.session_file.unwrap() // safe to unwrap because local_session_id is Some
        } else {
            let session_dir = self.volume_mgr.open_dir(self.sessions_dir, format!("{}", local_id).as_str()).unwrap();
            self.volume_mgr.open_file_in_dir(session_dir, "SESSION.TSF", Mode::ReadOnly).unwrap()
        };

        // Skip the start time
        let size = self.volume_mgr.file_length(file).unwrap();
        (size - 8) as usize / ENCODED_LENGTH
    }

    pub fn get_local_session_id(&self) -> Option<u32> {
        self.local_session_id
    }

    pub fn read_session_start_timestamp(&mut self, local_id: u32) -> i64 {
        let file = if self.local_session_id == Some(local_id) {
            self.session_file.unwrap() // safe to unwrap because local_session_id is Some
        } else {
            let session_dir = self.volume_mgr.open_dir(self.sessions_dir, format!("{}", local_id).as_str()).unwrap();
            self.volume_mgr.open_file_in_dir(session_dir, "SESSION.TSF", Mode::ReadOnly).unwrap()
        };

        let mut file = file.to_file(&mut self.volume_mgr);

        // Timestamp
        file.seek_from_start(0).unwrap();
        let mut buffer = [0; 8];
        file.read(&mut buffer).unwrap();

        i64::from_be_bytes(buffer)
    }

    /// Returns the start time and the requested points
    pub fn read_track_points(&mut self, local_id: u32, idx: usize, count: usize) -> Vec<u8> {
        let file = if self.local_session_id == Some(local_id) {
            self.session_file.unwrap() // safe to unwrap because local_session_id is Some
        } else {
            let session_dir = self.volume_mgr.open_dir(self.sessions_dir, format!("{}", local_id).as_str()).unwrap();
            self.volume_mgr.open_file_in_dir(session_dir, "SESSION.TSF", Mode::ReadOnly).unwrap()
        };

        let mut file = file.to_file(&mut self.volume_mgr);

        // Track points
        let mut buffer = [0; ENCODED_LENGTH];
        let mut data_bytes = Vec::with_capacity(count);

        let start_offset = 8 + idx * ENCODED_LENGTH;
        file.seek_from_start(start_offset as u32).unwrap();
        
        for _ in 0..count {
            file.read(&mut buffer).unwrap();
            data_bytes.extend_from_slice(&buffer);
        }

        data_bytes
    }

    pub fn read_upload_status(&mut self) -> UploadStatus {
        let upload_state_str = self.read_file_as_str(self.config_file);
        if upload_state_str.is_empty() {
            return UploadStatus::default();
        }
        let state = UploadStatus::parse(&upload_state_str);

        debug!("Upload status: {:?}", state);

        state
    }

    pub fn write_upload_status(&mut self, state: UploadStatus) {
        let len = self.volume_mgr.file_length(self.session_file.unwrap()).unwrap();
        let mut buffer = itoa::Buffer::new();
        let mut written_bytes = 0;

        let header = "local_id,remote_id,uploaded\n";
        self.volume_mgr.write(self.upload_status_file, header.as_bytes()).unwrap();
        written_bytes += header.len();

        for state in state.sessions {
            let local_id_bytes = buffer.format(state.local_id).as_bytes();
            self.volume_mgr.write(self.upload_status_file, local_id_bytes).unwrap();
            written_bytes += local_id_bytes.len();

            self.volume_mgr.write(self.upload_status_file, b",").unwrap();
            written_bytes += 1;

            if let Some(remote_id) = state.remote_id {
                let remote_id_bytes = buffer.format(remote_id).as_bytes();
                self.volume_mgr.write(self.upload_status_file, remote_id_bytes).unwrap();
                written_bytes += remote_id_bytes.len();
            } else {
                self.volume_mgr.write(self.upload_status_file, b"?").unwrap();
                written_bytes += 1;
            }

            self.volume_mgr.write(self.upload_status_file, b",").unwrap();
            written_bytes += 1;

            let count_bytes = buffer.format(state.uploaded).as_bytes();
            self.volume_mgr.write(self.upload_status_file, count_bytes).unwrap();
            written_bytes += count_bytes.len();

            self.volume_mgr.write(self.upload_status_file, b"\n").unwrap();
            written_bytes += 1;
        }

        // Pad rest of file with zeros to avoid closing, deleting and recreating the file
        if written_bytes < len as usize {
            self.volume_mgr.write(self.upload_status_file, &vec![0; len as usize - written_bytes]).unwrap();
        }

        self.volume_mgr.flush_file(self.upload_status_file).unwrap();
    }

    pub fn read_config(&mut self) -> Configuration {
        let config_str = self.read_file_as_str(self.config_file);
        let cfg = Configuration::parse(&config_str);

        debug!("Config: {:?}", cfg);

        cfg
    }

    fn read_file_as_str(&mut self, file: RawFile) -> String {
        let mut file = file.to_file(&mut self.volume_mgr);
        let file_size = file.length();
        let mut buffer = Vec::with_capacity(file_size as usize);
        file.read(&mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }

    pub fn initialize(
        spi:    PeripheralRef<'static, AnySpi>,
        sclk:   PeripheralRef<'static, AnyPin>,
        miso:   PeripheralRef<'static, AnyPin>,
        mosi:   PeripheralRef<'static, AnyPin>,
        cs:     PeripheralRef<'static, AnyPin>
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

        let Ok(config_file) = volume_mgr.open_file_in_dir(root_dir, "CONFIG.CFG", Mode::ReadOnly) else {
            error!("No config file found"); // Maybe write a default?
            panic!();
        };

        let Ok(upload_status_file) = volume_mgr.open_file_in_dir(root_dir, "STATE.CSV", Mode::ReadWriteCreate) else {
            panic!("No STATE.CSV file found");
        };

        let Ok(sys_log_file) = volume_mgr.open_file_in_dir(root_dir, "SYSTEM.LOG", Mode::ReadWriteCreateOrAppend) else {
            panic!("No SYSTEM.LOG file found");
        };

        if volume_mgr.find_directory_entry(root_dir, "SESSIONS").is_err() {
            volume_mgr.make_dir_in_dir(root_dir, "SESSIONS").unwrap();
        }

        let sessions_dir = volume_mgr.open_dir(root_dir, "SESSIONS").unwrap();

        Self {
            volume_mgr,

            root_dir,
            config_file,
            upload_status_file,
            sys_log_file,
            sessions_dir,

            local_session_id: None,
            start_time: None,
            session_file: None,
            session_log_file: None,
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