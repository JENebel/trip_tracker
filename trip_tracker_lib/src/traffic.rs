use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlx")]
use sqlx::FromRow;

#[cfg_attr(feature = "sqlx", derive(FromRow))]
#[derive(Serialize, Deserialize)]
pub struct Visit {
    pub ip: String,
    pub timestamp: DateTime<Utc>,
}

#[cfg_attr(feature = "sqlx", derive(FromRow))]
#[derive(Serialize, Deserialize, Clone)]
pub struct IpInfo {
    pub ip: String,
    pub country: String, // 2 letter country code
    pub latitude: f32,
    pub longitude: f32,
}

#[derive(Serialize, Deserialize)]
pub struct SiteTrafficData {
    pub visits: Vec<Visit>,
    pub ip_info: HashMap<String, IpInfo>,
}