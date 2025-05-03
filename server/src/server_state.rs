use std::net::IpAddr;

use tokio::sync::broadcast;
use trip_tracker_data_management::DataManager;

pub struct ServerState {
    // Channel used to send messages to all connected clients.
    pub tx: broadcast::Sender<String>,
    pub data_manager: DataManager,
    pub ip_address: IpAddr,
}