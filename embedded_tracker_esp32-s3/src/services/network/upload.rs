use core::fmt::{self, Debug};

use alloc::{format, boxed::Box};
use embassy_time::Timer;
use esp_println::println;
use heapless::Vec;

use crate::{info, ExclusiveService, ModemService, Service, StorageService};


pub enum UploadState {
    Current(usize),
    FromStorage(usize),
}

pub struct UploadService {
    modem_service: ExclusiveService<ModemService>,
    storage_service: ExclusiveService<StorageService>,

    upload_states: Vec<UploadState, 10>,
}

impl Debug for UploadService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UploadService {{  }}")
    }
}

#[async_trait::async_trait]
impl Service for UploadService {
    async fn start(&mut self) {
        self.connect().await;
    }

    async fn stop(&mut self) {
        
    }
}

impl UploadService {
    pub async fn initialize(
        modem_service: ExclusiveService<ModemService>,
        storage_service: ExclusiveService<StorageService>,
    ) -> Self {
        let s = Self {
            modem_service,
            storage_service,
            upload_states: Vec::new(),
        };

        s.setup_network().await;
        
        s
    }

    async fn setup_network(&self) {
        /*let res = modem.interrogate_timeout("AT+CREG", 5000).await.unwrap(); // Network registration
        println!("{}", res);*/
    
        //let res = modem.interrogate("AT+CFUN=1").await;
        //println!("CFUN: {:?}", res);
    
        //let res = modem.interrogate("AT+CPIN?").await;
        //println!("CPIN?: {:?}", res);

        let mut modem = self.modem_service.lock().await;
    
        /*let res = modem.interrogate_timeout("AT+NETCLOSE", 5000).await;
        println!("NETCLOSE: {:?}", res);
        Timer::after_millis(5000).await;*/
    
        // AT+CPIN if required/present
        let res = {
            let storage_service = self.storage_service.lock().await;
            let config = storage_service.get_config();
            modem.interrogate_timeout(&format!("AT+CGAUTH=1,0,{:?},{:?}", config.apn_user, config.apn_password), 5000).await
        };
        info!("CGAUTH: {:?}", res);
    
        let res = modem.interrogate(&format!("AT+CGDCONT= 1,\"IP\",{:?},0,0", self.storage_service.lock().await.get_config().apn)).await;
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
        info!("NETOPEN: {:?}", res);

        /*let res = modem.interrogate_urc("AT+CIPRXGET=1", "+CIPRXGET", 5000).await;
        info!("CIPRXGET: {:?}", res);*/
    }

    async fn connect(&self) {
        let mut modem = self.modem_service.lock().await;

        let res = modem.interrogate_urc("AT+NETOPEN", "+NETOPEN", 10000).await;
        info!("NETOPEN: {:?}", res);

        let sub = modem.subscribe_to_urc("+CIPRXGET").await;

        {
            let storage_service = self.storage_service.lock().await;
            let config = storage_service.get_config();
            let res = modem.interrogate_urc(&format!("AT+CIPOPEN=0,\"TCP\",{},{}", config.server, config.port), "+CIPOPEN", 10000).await;
            info!("CIPOPEN: {:?}", res);
        }

        /*Timer::after_millis(1000).await;

        // Read nonce as HEX
        let res = modem.interrogate_urc("AT+CIPRXGET=3,0", "+CIPRXGET", 25000).await;
        info!("CIPRXGET: {:?}", res);

        let sub = modem.subscribe_to_urc("+CIPRXGET").await;
        let res = sub.receive().await;
        info!("CIPRXGET: {:?}", res);*/
    }
}