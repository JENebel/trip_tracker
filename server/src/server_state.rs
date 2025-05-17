use std::{collections::HashMap, net::IpAddr};

use tokio::sync::{broadcast, Mutex};
use data_management::DataManager;

pub struct ServerState {
    // Channel used to send messages to all connected clients.
    pub tx: broadcast::Sender<String>,
    pub data_manager: DataManager,
    pub ip_address: IpAddr,
    pub ip_load: Mutex<HashMap<IpAddr, usize>>,
}