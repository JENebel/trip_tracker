use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Trip {
    pub trip_id: i64,
    pub timestamp: DateTime<Utc>,
    pub title: String,
    pub description: String,
    pub api_token: String,
}

impl Trip {
    pub fn new(trip_id: i64, title: String, description: String, timestamp: DateTime<Utc>, api_token: String) -> Self {
        Self {
            trip_id,
            timestamp,
            title,
            description,
            api_token,
        }
    }
}