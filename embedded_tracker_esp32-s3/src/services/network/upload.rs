use heapless::Vec;

use crate::{info, ExclusiveService, ModemService, StorageService};


pub enum UploadState {
    Current(usize),
    FromStorage(usize),
}

pub struct UploadService {
    modem_service: ExclusiveService<ModemService>,
    storage_service: ExclusiveService<StorageService>,

    upload_states: Vec<UploadState, 10>,
}

impl UploadService {
    async fn setup_network(&self) {
        /*let res = modem.interrogate_timeout("AT+CREG", 5000).await.unwrap(); // Network registration
        println!("{}", res);*/
    
        //let res = modem.interrogate("AT+CFUN=1").await;
        //println!("CFUN: {:?}", res);
    
        //let res = modem.interrogate("AT+CPIN?").await;
        //println!("CPIN?: {:?}", res);
    
        let mut modem = self.modem_service.lock().await;
    
        info!("CFUN...");
        let res = modem.interrogate_urc("AT+CFUN=?", "+CFUN", 25000).await;
        info!("{:?}", res);
    
        /*let res = modem.interrogate_timeout("AT+NETCLOSE", 5000).await;
        println!("NETCLOSE: {:?}", res);
        Timer::after_millis(5000).await;*/
    
        // AT+CPIN if required/present
        let user = "";
        let pass = "";
        let res = modem.interrogate_timeout("AT+CGAUTH=1,0,\"\",\"\"", 5000).await;
        info!("CGAUTH: {:?}", res);
    
        let apn = "internet";
        let res = modem.interrogate("AT+CGDCONT= 1,\"IP\",\"internet\",0,0").await;
        info!("CGDCONT: {:?}", res);
    
        let res = modem.interrogate("AT+CSQ").await;
        info!("CSQ?: {:?}", res);
    
        let res = modem.interrogate("AT+CIPCCFG=10,0,0,0,1,0,500").await;
        info!("CIPCCFG: {:?}", res);
    
        let res = modem.interrogate("AT+CIPTIMEOUT=5000,1000,1000").await;
        info!("CIPTIMEOUT: {:?}", res);
    
        let res = modem.interrogate("AT+CGACT=1,1").await;
        info!("CGACT: {:?}", res);
        
        let res = modem.interrogate("AT+NETOPEN").await;
        info!("NETOPEN: {:?}", res);
    
        let res = modem.interrogate("AT+CPSI?").await;
        info!("CPSI: {:?}", res);
    
        let res = modem.interrogate("AT+CPING=\"www.google.com\" ,1").await;
        info!("CPING: {:?}", res);
    }
}