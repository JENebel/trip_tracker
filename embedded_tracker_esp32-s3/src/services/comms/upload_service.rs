use core::fmt::{self, Debug};

use alloc::{boxed::Box, format, sync::Arc};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Duration;
use esp_hal::sha::Sha;
use esp_println::println;
use trip_tracker_lib::comms::{HandshakeMessage, MacProvider, SIGNATURE_SIZE};

use crate::{info, services::modem::modem_service::ATResponse, ActorControl, Configuration, ExclusiveService, ModemService, Service, StorageService};

use super::{mac_provider::EmbeddedMacProvider, upload_status::{SessionUploadStatus, UploadStatus}};

pub struct UploadService {
    modem_service: ExclusiveService<ModemService>,
    storage_service: ExclusiveService<StorageService>,

    upload_status: Arc<Mutex<CriticalSectionRawMutex, UploadStatus>>,

    actor_control: ActorControl,
}

impl Debug for UploadService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UploadService {{  }}")
    }
}

#[async_trait::async_trait]
impl Service for UploadService {
    async fn start(&mut self) {
        self.actor_control.start().await;
    }

    async fn stop(&mut self) {
        self.actor_control.stop().await;
    }
}

impl UploadService {
    pub async fn initialize(
        spawner: &Spawner,
        sha: Sha<'static>,
        modem_service: ExclusiveService<ModemService>,
        storage_service: ExclusiveService<StorageService>,
    ) -> Self {
        let upload_status = storage_service.lock().await.read_upload_status();
        let upload_status = Arc::new(Mutex::new(upload_status));

        let actor_control = ActorControl::new();

        let mac_provider = Arc::new(Mutex::new(EmbeddedMacProvider::new(sha)));

        spawner.must_spawn(upload_actor(
            mac_provider.clone(),
            upload_status.clone(),
            modem_service.clone(),
            storage_service.clone(),
            actor_control.clone(),
        ));

        let s = Self {
            modem_service,
            storage_service,
            upload_status,
            actor_control,
        };

        s.setup_network().await;
        
        s
    }

    pub async fn add_active_session(&self, local_id: u32) {
        let mut upload_status = self.upload_status.lock().await;
        upload_status.add_session(local_id);
        self.storage_service.lock().await.write_upload_status(upload_status.clone());
    }

    async fn setup_network(&self) {
        let mut modem = self.modem_service.lock().await;

        let config = self.storage_service.lock().await.get_config();
    
        // AT+CPIN if required/present
        let res = {
            modem.interrogate_timeout(&format!("AT+CGAUTH=1,0,{:?},{:?}", config.apn_user, config.apn_password), 5000).await
        };
        info!("CGAUTH: {:?}", res);
    
        let res = modem.interrogate(&format!("AT+CGDCONT= 1,\"IP\",{:?},0,0", config.apn)).await;
        info!("CGDCONT: {:?}", res);
    
        let res = modem.interrogate("AT+CSQ").await;
        info!("CSQ?: {:?}", res);
    
        let res = modem.interrogate("AT+CIPCCFG=10,0,0,0,1,0,500").await;
        info!("CIPCCFG: {:?}", res);
    
        let res = modem.interrogate("AT+CIPTIMEOUT=5000,1000,1000").await;
        info!("CIPTIMEOUT: {:?}", res);
    
        let res = modem.interrogate("AT+CGACT=1,1").await;
        info!("CGACT: {:?}", res);

        let res = modem.interrogate("AT+CIPSRIP=0").await;
        info!("CIPSRIP: {:?}", res);

        let res = modem.interrogate_urc("AT+NETOPEN", "+NETOPEN", 10000).await;
        info!("NETOPEN: {:?}", res);
    }
}

// Aim to upload data every 5 secs
const UPLOAD_INTERVAL: Duration = Duration::from_secs(5);

#[embassy_executor::task]
async fn upload_actor(
    mac_provider: Arc<Mutex<CriticalSectionRawMutex, EmbeddedMacProvider>>,
    upload_status: Arc<Mutex<CriticalSectionRawMutex, UploadStatus>>,
    modem_service: ExclusiveService<ModemService>,
    storage_service: ExclusiveService<StorageService>,
    actor_control: ActorControl,
) {
    let wait_interval;
    let mut connection_session_id = None;

    loop {
        actor_control.wait_for_start().await;
        wait_interval = UPLOAD_INTERVAL;

        let config = storage_service.lock().await.get_config();

        loop {
            let active_session_id = storage_service.lock().await.get_local_session_id();

            // Start by uploading old unfinished session data
            let status_clone = upload_status.lock().await.clone();
            for session in status_clone.sessions.iter() {
                let track_point_count = storage_service.lock().await.get_session_track_point_count(session.local_id);
                let missing = track_point_count - session.uploaded;

                if missing > 0 {
                    if connection_session_id.is_none() || connection_session_id != Some(session.local_id) {
                        // Start new connection with this id

                        // If already connected, disconnect
                        if connection_session_id.is_some() {
                            modem_service.lock().await.send("AT+CIPCLOSE=0").await.unwrap();
                        }

                        if let Some(remote_id) = session.remote_id {
                            connect(&mut *modem_service.lock().await, ConnectStrategy::Reconnect(remote_id), &config, &mut *mac_provider.lock().await).await;
                        } else {
                            let start_time = storage_service.lock().await.read_session_start_timestamp(session.local_id);
                            let session_id = connect(&mut *modem_service.lock().await, ConnectStrategy::Connect(start_time), &config, &mut *mac_provider.lock().await).await;
                            upload_status.lock().await.set_remote_session_id(session.local_id, session_id);
                            storage_service.lock().await.write_upload_status(upload_status.lock().await.clone());
                        }

                        connection_session_id = Some(session.local_id);
                    }

                    upload_data(
                        session, 
                        mac_provider.clone(), 
                        &config, 
                        missing, 
                        modem_service.clone(), 
                        storage_service.clone()
                    ).await;

                    info!("Uploaded {} points", missing);

                    upload_status.lock().await.add_uploaded(session.local_id, missing);
                    storage_service.lock().await.write_upload_status(upload_status.lock().await.clone());
                }

                // If no data is pending, and it is an inactive session OR we are in the process of stopping, then finish the session.
                // If we are currently in the process of stopping, there will not com any more data, so we can safely wrap up.
                if missing == 0 && (active_session_id != Some(session.local_id) || !actor_control.is_running().await) {
                    // Old session is finished!
                    upload_status.lock().await.finish_session(session.local_id);
                    storage_service.lock().await.write_upload_status(upload_status.lock().await.clone());

                    info!("Session {} finished", session.local_id);
                }
            }

            embassy_time::Timer::after(wait_interval).await;
        }
    }
}

async fn upload_data(
    status: &SessionUploadStatus,
    mac_provider: Arc<Mutex<CriticalSectionRawMutex, EmbeddedMacProvider>>,
    config: &Configuration,
    mut missing: usize,
    modem_service: ExclusiveService<ModemService>,
    storage_service: ExclusiveService<StorageService>,
) {
    let mut idx = status.uploaded;
    while missing > 0 {
        let point_cnt = if missing > 250 {
            250
        } else {
            missing
        };

        let mut data = storage_service.lock().await.read_track_points(status.local_id, idx, point_cnt);
        idx += point_cnt;

        data.insert(0, point_cnt as u8);

        // Sign data
        let key = config.auth_key;
        let signature = mac_provider.lock().await.sign(&data, &key);
        data.extend_from_slice(&signature);

        let mut modem_service = modem_service.lock().await;

        let Ok(ATResponse::ReadyForInput) = modem_service.send(&format!("AT+CIPSEND=0,{}", data.len())).await else {
            panic!("Failed CIPSEND");
        };
    
        modem_service.send_bytes(&data).await.unwrap();

        missing -= point_cnt;
    }
}

#[derive(Debug)]
enum ConnectStrategy {
    Connect(i64), // timestamp
    Reconnect(i64), // session_id
}

async fn connect(modem: &mut ModemService, connect_strategy: ConnectStrategy, config: &Configuration, mac_provider: &mut EmbeddedMacProvider) -> i64 {
    info!("{:?} to {}:{}", connect_strategy, config.server, config.port);

    let res = modem.interrogate_urc(&format!("AT+CIPOPEN=0,\"TCP\",{},{}", config.server, config.port), "+CIPOPEN", 10000).await;
    info!("CIPOPEN: {:?}", res);

    let mut buffer = [0; 17 + SIGNATURE_SIZE];

    let mut nonce_buffer = [0; 16];
    let receive_buffer = modem.get_receive_data_buffer(0);
    receive_buffer.read_exact(&mut nonce_buffer).await;
    println!("Nonce: {:?}", &nonce_buffer[..16]);

    let handshake_message = match connect_strategy {
        ConnectStrategy::Connect(timestamp) => HandshakeMessage::new_fresh(config.trip_id, timestamp),
        ConnectStrategy::Reconnect(session_id) => HandshakeMessage::new_reconnect(config.trip_id, session_id),
    };
    let handshake_bytes = handshake_message.serialize();
    buffer[..17].copy_from_slice(&handshake_bytes);

    let mut to_sign = [0u8; 16 + 17];
    to_sign[..16].copy_from_slice(&nonce_buffer);
    to_sign[16..].copy_from_slice(&handshake_bytes);

    let signature = mac_provider.sign(&to_sign, &config.auth_key);

    println!("Signing");
    println!("Data: {:?}", &to_sign);
    println!("Signature: {:?}", &signature);
    println!("Key: {:?}", &config.auth_key);

    buffer[..17].copy_from_slice(&handshake_bytes);
    buffer[17..].copy_from_slice(&signature);

    let Ok(ATResponse::ReadyForInput) = modem.send(&format!("AT+CIPSEND=0,{}", buffer.len())).await else {
        panic!("Failed CIPSEND");
    };

    modem.send_bytes(&buffer).await.unwrap();

    // If fresh connection, read session id
    let session_id = match connect_strategy {
        ConnectStrategy::Reconnect(session_id) => session_id,
        ConnectStrategy::Connect(_) => {
            let mut session_id_buffer = [0; 8];
            receive_buffer.read_exact(&mut session_id_buffer).await;
            let session_id = i64::from_be_bytes(session_id_buffer);
            println!("Got fresh session ID: {}", session_id);
            session_id
        },
    };

    info!("Succesfully connected to server");

    session_id
}