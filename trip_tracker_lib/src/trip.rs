use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
#[derive(Serialize, Deserialize)]
pub struct Trip {
    pub trip_id: i64,
    pub user_id: i64,
    pub name: String,
    pub active: bool,
    pub start_time: chrono::DateTime<chrono::Utc>,
}