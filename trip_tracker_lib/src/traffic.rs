use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Serialize, Deserialize, FromRow)]
pub struct Visit {
    pub timestamp: DateTime<Utc>,
    pub ip: [u8; 4],
}