use core::fmt::{self, Debug};

use alloc::{boxed::Box, format, sync::Arc};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use esp_hal::{gpio::AnyPin, sha::Sha};
use trip_tracker_lib::comms::{HandshakeMessage, MacProvider, MAX_TRACK_POINTS_PER_MESSAGE, SIGNATURE_SIZE};

use crate::{debug, info, services::modem::modem_service::{ATError, ATErrorType}, warn, ActorControl, Configuration, ExclusiveService, ModemService, Service, StorageService};

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
        while self.modem_service.lock().await.send("AT+NETOPEN").await.is_err() {};
        self.actor_control.start().await;
    }

    async fn stop(&mut self) {
        self.actor_control.stop().await;
        self.modem_service.lock().await.send("AT+NETCLOSE").await.unwrap();
    }
}

impl UploadService {
    pub async fn initialize(
        spawner: &Spawner,
        sha: Sha<'static>,
        modem_service: ExclusiveService<ModemService>,
        storage_service: ExclusiveService<StorageService>,
        led: esp_hal::peripheral::PeripheralRef<'static, AnyPin>, 
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
            led,
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
        self.storage_service.lock().await.write_upload_status(&upload_status);
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
    
        let res = modem.interrogate("AT+CIPCCFG=10,0,0,0,1,0,500").await;
        info!("CIPCCFG: {:?}", res);
    
        let res = modem.interrogate("AT+CIPTIMEOUT=3000,3000,3000").await; // Minimum for (netopen, cipopen, cipsend)
        info!("CIPTIMEOUT: {:?}", res);
    
        let res = modem.interrogate("AT+CGACT=1,1").await;
        info!("CGACT: {:?}", res);

        let res = modem.interrogate("AT+CIPSRIP=0").await;
        info!("CIPSRIP: {:?}", res);
    }
}

// Aim to upload data every 6 secs
const UPLOAD_INTERVAL: Duration = Duration::from_secs(6);
const RETRIES_AFTER_STOP: usize = 100; // 10 minutes max after stop

#[embassy_executor::task]
async fn upload_actor(
    mac_provider: Arc<Mutex<CriticalSectionRawMutex, EmbeddedMacProvider>>,
    upload_status: Arc<Mutex<CriticalSectionRawMutex, UploadStatus>>,
    modem_service: ExclusiveService<ModemService>,
    storage_service: ExclusiveService<StorageService>,
    actor_control: ActorControl,
    led: esp_hal::peripheral::PeripheralRef<'static, AnyPin>, 
) {
    let mut led = esp_hal::gpio::Output::new(led, esp_hal::gpio::Level::Low);

    loop {
        if !actor_control.is_running() {
            led.set_low();
            actor_control.stopped();
            debug!("Upload service stopped");

            actor_control.wait_for_start().await;
        }

        // Ensure no connection
        let mut connected_session_id = None;

        let config = storage_service.lock().await.get_config();
        let active_session_id = storage_service.lock().await.get_local_session_id();

        let mut finish_retries_left = RETRIES_AFTER_STOP;

        loop {
            Timer::after(UPLOAD_INTERVAL).await;

            let res = modem_service.lock().await.interrogate_urc("AT+CSQ", "+CSQ", 1000).await;
            info!("CSQ?: {:?}", res);

            // Start by uploading old unfinished session data
            let status_clone = upload_status.lock().await.clone();

            let result: Result<(), ATError> = (async || {
                for session in status_clone.sessions.iter() {
                    let track_point_count = storage_service.lock().await.get_session_track_point_count(session.local_id);
                    let missing = track_point_count - session.uploaded;

                    if connected_session_id.is_none() || connected_session_id != Some(session.local_id) {
                        // Start new connection with this id
                        ensure_closed(&modem_service).await;

                        if let Some(remote_id) = session.remote_id {
                            connect(
                                modem_service.clone(), 
                                ConnectStrategy::Reconnect(remote_id), 
                                &config, 
                                &mut *mac_provider.lock().await
                            ).await?;
                        } else {
                            let start_time = storage_service.lock().await.read_session_start_timestamp(session.local_id);
                            let session_id = connect(
                                modem_service.clone(), 
                                ConnectStrategy::Connect(start_time), 
                                &config, 
                                &mut *mac_provider.lock().await
                            ).await?;
                            upload_status.lock().await.set_remote_session_id(session.local_id, session_id);
                            storage_service.lock().await.write_upload_status(&*upload_status.lock().await);
                        }

                        info!("Succesfully connected to server");

                        connected_session_id = Some(session.local_id);
                    }

                    if missing > 0 {
                        upload_data(
                            session, 
                            mac_provider.clone(), 
                            &config, 
                            missing, 
                            modem_service.clone(), 
                            storage_service.clone()
                        ).await?;

                        info!("Uploaded {} points", missing);

                        upload_status.lock().await.add_uploaded(session.local_id, missing);
                        storage_service.lock().await.write_upload_status(&*upload_status.lock().await);
                    }

                    // Missing is now 0
                    let not_current_session = active_session_id != Some(session.local_id);
                    if !actor_control.is_running() || not_current_session {
                        finish_session(session, upload_status.clone(), storage_service.clone(), modem_service.clone(), &mut *mac_provider.lock().await).await?;
                        ensure_closed(&modem_service).await;
                        info!("Session {} finished", session.local_id);
                    }
                }

                Ok(())
            })().await;

            if let Err(e) = result {
                warn!("Failed to upload data: {:?}", e);

                connected_session_id = None;
                led.set_low();
            } else {
                led.set_high();
            }

            if !actor_control.is_running() {
                info!("Upload service stopped, waiting for all sessions to finish");
                if upload_status.lock().await.get_session_count() == 0 {
                    led.set_low();
                    info!("All sessions uploaded, stopping upload service");
                    break;
                }

                finish_retries_left -= 1;

                if finish_retries_left == 0 {
                    led.set_low();
                    info!("All sessions not finished, stopping upload service");
                    break;
                }
            }
        }
    }
}

async fn finish_session(
    session: &SessionUploadStatus,
    upload_status: Arc<Mutex<CriticalSectionRawMutex, UploadStatus>>,
    storage_service: ExclusiveService<StorageService>,
    modem_service: ExclusiveService<ModemService>,
    mac_provider: &mut EmbeddedMacProvider,
) -> Result<(), ATError> {
    // Send single 0 byte to finish session
    modem_service.lock().await.cip_send_bytes::<0>(&[0]).await?;

    // Receive nonce 
    let mut nonce_buffer = [0; 16];
    let receive_buffer = modem_service.lock().await.get_receive_data_buffer(0);
    receive_buffer.read_exact_timeout(&mut nonce_buffer, 3000).await.map_err(|_| ATError::new(ATErrorType::Timeout, "Receive nonce timed out"))?;

    // Sign nonce
    let key = storage_service.lock().await.get_config().auth_key;
    let signature = mac_provider.sign(&nonce_buffer, &key);

    // Send signature
    modem_service.lock().await.cip_send_bytes::<0>(&signature).await?;

    // Read response byte
    let mut response = [0; 1];
    receive_buffer.read_exact_timeout(&mut response, 3000).await.map_err(|_| ATError::new(ATErrorType::Timeout, "Receive finish response timed out"))?;
    if response[0] != 1 {
        return Err(ATError::new(ATErrorType::TxError, &format!("Finish response not 1! Got {}", response[0])));
    }

    // Old session is finished!
    upload_status.lock().await.finish_session(session.local_id);
    storage_service.lock().await.write_upload_status(&*upload_status.lock().await);

    Ok(())
}

async fn upload_data(
    status: &SessionUploadStatus,
    mac_provider: Arc<Mutex<CriticalSectionRawMutex, EmbeddedMacProvider>>,
    config: &Configuration,
    mut missing: usize,
    modem_service: ExclusiveService<ModemService>,
    storage_service: ExclusiveService<StorageService>,
) -> Result<(), ATError> {
    let mut idx = status.uploaded;
    while missing > 0 {
        let point_cnt = if missing > MAX_TRACK_POINTS_PER_MESSAGE {
            MAX_TRACK_POINTS_PER_MESSAGE
        } else {
            missing
        };

        info!("Uploading {} points", point_cnt);

        let mut data = storage_service.lock().await.read_track_points(status.local_id, idx, point_cnt);
        idx += point_cnt;

        data.insert(0, point_cnt as u8);

        // Sign data
        let key = config.auth_key;
        let signature = mac_provider.lock().await.sign(&data, &key);
        data.extend_from_slice(&signature);

        modem_service.lock().await.cip_send_bytes::<0>(&data).await?;

        missing -= point_cnt;
    }

    Ok(())
}

#[derive(Debug)]
enum ConnectStrategy {
    Connect(i64), // timestamp
    Reconnect(i64), // session_id
}

#[derive(Debug, PartialEq)]
enum NetError {
    Succes,
    NetworkFailure,
    NetworkNotOpened,
    WrongParameter,
    OperationNotSuported,
    FailedToCreateSocket,
    FailedToBindSocket,
    TCPServerIsAlreadyListening,
    Busy,
    SocketsOpened,
    Timeout,
    DNSParseFailed,
    Unknown,
}

impl NetError {
    fn from_code(code: &str) -> Self {
        match code {
            "0" => NetError::Succes,
            "1" => NetError::NetworkFailure,
            "2" => NetError::NetworkNotOpened,
            "3" => NetError::WrongParameter,
            "4" => NetError::OperationNotSuported,
            "5" => NetError::FailedToCreateSocket,
            "6" => NetError::FailedToBindSocket,
            "7" => NetError::TCPServerIsAlreadyListening,
            "8" => NetError::Busy,
            "9" => NetError::SocketsOpened,
            "10" => NetError::Timeout,
            "11" => NetError::DNSParseFailed,
            "12" => NetError::Unknown,
            _ => unreachable!("These are the only possible error codes"),
        }
    }
}

async fn ensure_closed(modem_service: &ExclusiveService<ModemService>) {
    let _ = modem_service.lock().await.interrogate_urc("AT+CIPCLOSE=0", "+CIPCLOSE", 3500).await;
}

async fn connect(
    modem_service: ExclusiveService<ModemService>, 
    connect_strategy: ConnectStrategy, 
    config: &Configuration, 
    mac_provider: &mut EmbeddedMacProvider
) -> Result<i64, ATError> {
    info!("{:?} to {}:{}", connect_strategy, config.server, config.port);

    let res = modem_service.lock().await.send("AT+NETOPEN?").await;
    info!("NETOPEN?: {:?}", res);

    let command = format!("AT+CIPOPEN=0,\"TCP\",\"{}\",{}", config.server, config.port);
    let res = modem_service.lock().await.interrogate_urc(&command, "+CIPOPEN", 3000).await?;
    debug!("{:?}", res);
    let code = res.1.split_once(',').unwrap().1;
    let code = NetError::from_code(code);
    if code != NetError::Succes {
        return Err(ATError::new(ATErrorType::NetError(format!("{:?}", code)), &command));
    }

    let mut buffer = [0; 17 + SIGNATURE_SIZE];

    let mut nonce_buffer = [0; 16];
    let receive_buffer = modem_service.lock().await.get_receive_data_buffer(0);
    receive_buffer.read_exact_timeout(&mut nonce_buffer, 3000).await.map_err(|_| ATError::new(ATErrorType::Timeout, "Receive connect nonce timed out"))?;

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

    buffer[..17].copy_from_slice(&handshake_bytes);
    buffer[17..].copy_from_slice(&signature);

    modem_service.lock().await.cip_send_bytes::<0>(&buffer).await?;

    // If fresh connection, read session id
    let session_id = match connect_strategy {
        ConnectStrategy::Reconnect(session_id) => session_id,
        ConnectStrategy::Connect(_) => {
            let mut session_id_buffer = [0; 8];
            receive_buffer.read_exact_timeout(&mut session_id_buffer, 3000).await.map_err(|_| ATError::new(ATErrorType::Timeout, "Receive new session ID timed out"))?;
            let session_id = i64::from_be_bytes(session_id_buffer);
            session_id
        },
    };

    Ok(session_id)
}