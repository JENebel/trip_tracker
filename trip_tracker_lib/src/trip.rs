use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::{prelude::*, sqlite::SqliteRow};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Trip {
    pub trip_id: i64,
    pub timestamp: DateTime<Utc>,
    pub title: String,
    pub description: String,
    pub api_token: String,
    pub country_list: Vec<String>,
}

#[cfg(feature = "sqlx")]
impl FromRow<'_, SqliteRow> for Trip {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let country_codes: Vec<u8> = row.get(5);
        let country_list = if country_codes.is_empty() {
            Vec::new()
        } else {
            bincode::deserialize::<Vec<String>>(&country_codes).unwrap()
        };

        Ok(Self {
            trip_id: row.get(0),
            timestamp: row.get(1),
            title: row.get(2),
            description: row.get(3),
            api_token: row.get(4),
            country_list,
        })
    }
}

impl Trip {
    pub fn new(trip_id: i64, title: String, description: String, timestamp: DateTime<Utc>, api_token: String) -> Self {
        Self {
            trip_id,
            timestamp,
            title,
            description,
            api_token,
            country_list: Vec::new(),
        }
    }
}