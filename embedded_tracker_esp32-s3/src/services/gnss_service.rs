use core::fmt::{self, Debug};

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Instant;
use trip_tracker_lib::track_point::TrackPoint;

use crate::{info, services::modem::ModemService, warn, ActorTerminator, ExclusiveService, Service};

use alloc::{boxed::Box, sync::Arc};

use super::{state_service, StateService, StorageService, UploadService};

pub struct GNSSService {
    modem_service: ExclusiveService<ModemService>,
    gnss_actor: ActorTerminator,
}

#[async_trait::async_trait]
impl Service for GNSSService {

    async fn stop(&mut self) {
        self.disable_gnss().await;
        self.gnss_actor.terminate().await;
        self.modem_service.lock().await.send_timeout("AT+CGNSSPWR=0", 10000).await.unwrap();
    }
}

impl Debug for GNSSService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GNSS Service")
    }
}

impl GNSSService {
    pub async fn start(
        spawner: &Spawner, 
        storage_service: ExclusiveService<StorageService>,
        modem_service: ExclusiveService<ModemService>, 
        upload_service: ExclusiveService<UploadService>,
        state_service: ExclusiveService<StateService>,
    ) -> Self {
        let start_time = Arc::new(Mutex::new(None));
        let latest_state = Arc::new(Mutex::new(None));

        let terminator = ActorTerminator::new();

        spawner.must_spawn(gnss_monitor_actor(
            storage_service.clone(), 
            modem_service.clone(), 
            upload_service.clone(),
            state_service.clone(),
            start_time.clone(), 
            latest_state.clone(), 
            terminator.clone(),
        ));

        let mut gnss = Self {
            modem_service,
            gnss_actor: terminator,
        };

        gnss.enable_gnss().await;

        gnss
    }

    pub async fn enable_gnss(&mut self) {
        let mut modem = self.modem_service.lock().await;
        modem.send("AT+CGDRT=4,1").await.unwrap();
        modem.send("AT+CGSETV=4,1").await.unwrap();
        modem.send_timeout("AT+CGNSSPWR=1", 10000).await.unwrap();
        modem.send_timeout("AT+CGNSSMODE=15", 10000).await.unwrap(); // GPS + GLONASS + GALILEO + BDS
        modem.send_timeout("AT+CGNSSINFO=1", 10000).await.unwrap(); // Send GNSS info once every second
        modem.send_timeout("AT+CGNSSPORTSWITCH=1", 10000).await.unwrap();
    }

    pub async fn disable_gnss(&mut self) {
        //self.modem_service.lock().await.send_timeout("AT+CGNSSPWR=0", 10000).await.unwrap();
        self.modem_service.lock().await.send_timeout("AT+CGNSSINFO=0", 10000).await.unwrap(); // Disable send GNSS info once every second
    }
}

#[derive(Debug, Clone)]
pub struct GNSSState {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f32,
    pub timestamp: DateTime<Utc>,
    pub speed_kph: f32,
    pub course: f32,
    pub pdop: f32,
    pub hdop: f32,
    pub vdop: f32,
    pub satellites: u32,
    pub satellites_used: u32,
}

#[embassy_executor::task]
pub async fn gnss_monitor_actor(
    storage_service: ExclusiveService<StorageService>,
    modem_service: ExclusiveService<ModemService>,
    upload_service: ExclusiveService<UploadService>,
    state_service: ExclusiveService<StateService>,
    start_time: Arc<Mutex<CriticalSectionRawMutex, Option<DateTime<Utc>>>>,
    latest_state: Arc<Mutex<CriticalSectionRawMutex, Option<GNSSState>>>,
    terminator: ActorTerminator
) {
    let local_start_time = Instant::now();
    let mut has_recevied_data = false;
    let mut upload_initialized = false;

    let mut time_publisher = state_service::CURRENT_TIME.sender();
    let mut last_time_published = Instant::now();
    
    let gnss_subscriber = modem_service.lock().await.subscribe_to_urc("+CGNSSINFO").await;
    modem_service.lock().await.send_timeout("AT+CGNSSINFO", 10000).await.unwrap();

    loop {
        if terminator.is_terminating() {
            state_service.lock().await.set_gnss_state(false).await;
            terminator.terminated();
            break;
        }

        let Ok(gnss_info) = gnss_subscriber.receive(2000).await else {
            state_service.lock().await.set_gnss_state(false).await;
            continue;
        };

        let Some(state) = parse_gnss_info(&gnss_info).await else {
            continue;
        };

        if !has_recevied_data {
            info!("Time to fix: {:?} ms", (Instant::now() - local_start_time).as_millis());
            has_recevied_data = true;
            *start_time.lock().await = Some(state.timestamp);
            storage_service.lock().await.set_start_time(state.timestamp);
        }

        if !upload_initialized && state_service.lock().await.is_upload_enabled() {
            let local_id = storage_service.lock().await.get_local_session_id();
            upload_service.lock().await.add_active_session(local_id).await;
            upload_initialized = true;
        }

        time_publisher.send((state.timestamp, Instant::now()));

        let track_point = TrackPoint::new(
            state.timestamp,
            state.latitude,
            state.longitude,
            state.altitude,
            state.speed_kph,
            state.pdop < 1.
        );
        
        storage_service.lock().await.append_track_point(track_point);

        latest_state.lock().await.replace(state);

        state_service.lock().await.set_gnss_state(true).await;
    }
}

async fn parse_gnss_info(gnss_info: &str) -> Option<GNSSState> {
    let mut parts = gnss_info.split(",");

    let _mode: u8 = parts.next().unwrap().parse().ok()?;
    let gps_sats: u16 = parts.next().unwrap().parse().ok()?;
    let glonass_sats: u16 = parts.next().unwrap().parse().ok()?;
    let galileo_sats: u16 = parts.next().unwrap().parse().ok()?;
    let beidou_sats: u16 = parts.next().unwrap().parse().ok()?;
    let sats_total: u16 = gps_sats + glonass_sats + galileo_sats + beidou_sats;

    let latitude: f64 = parts.next().unwrap().parse().ok()?;
    let is_south = parts.next().unwrap() == "S";
    let latitude = latitude * if is_south { -1.0 } else { 1.0 };

    let longitude: f64 = parts.next().unwrap().parse().ok()?;
    let is_west = parts.next().unwrap() == "W";
    let longitude = longitude * if is_west { -1.0 } else { 1.0 };

    let date = parts.next().unwrap();
    let date = NaiveDate::parse_from_str(date, "%d%m%y").ok()?;
    let time = parts.next().unwrap();
    let time = NaiveTime::parse_from_str(time, "%H%M%S%.3f").ok()?;
    let datetime = NaiveDateTime::new(date, time).and_utc();

    let altitude: f32 = parts.next().unwrap().parse().ok()?;
    let speed_knots: f32 = parts.next().unwrap().parse().ok()?;
    let speed_kph = speed_knots * 1.852; // Knots to km/h
    if speed_kph > 500. {
        warn!("Speed is unrealistically high: {} km/h. Ignoring this point.", speed_kph);
        return None;
    }
    let _course: f32 = parts.next().unwrap().parse().ok()?;

    let pdop: f32 = parts.next().unwrap().parse().ok()?;
    let hdop: f32 = parts.next().unwrap().parse().ok()?;
    let vdop: f32 = parts.next().unwrap().parse().ok()?;

    let sats_used: u16 = parts.next().unwrap().parse().ok()?;

    let state = GNSSState {
        latitude,
        longitude,
        altitude,
        timestamp: datetime,
        speed_kph: speed_kph / 1.852,
        course: _course,
        pdop,
        hdop,
        vdop,
        satellites: sats_total as u32,
        satellites_used: sats_used as u32,
    };

    Some(state)
}