use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlx")]
use sqlx::{sqlite::SqliteRow, FromRow, Row};

use crate::track_point::parse_tsf;

use super::track_point::TrackPoint;

#[derive(Serialize, Deserialize)]
pub struct SessionUpdate {
    pub new_track_points: Vec<TrackPoint>,
    pub still_active: bool,
}

#[derive(Serialize, Deserialize)]
pub struct TrackSession {
    pub session_id: i64,
    pub trip_id: i64,
    pub start_time: DateTime<Utc>, // Handle non-utc time zones
    pub title: String,
    pub description: String,
    pub active: bool,
    pub track_points: Vec<TrackPoint>,
}

#[cfg(feature = "sqlx")]
impl FromRow<'_, SqliteRow> for TrackSession {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let track_point_bytes: Vec<u8> = row.get(6);
        let track_points = if track_point_bytes.is_empty() {
            Vec::new()
        } else {
            parse_tsf(&track_point_bytes).unwrap().0
        };

        Ok(Self {
            session_id: row.get(0),
            trip_id: row.get(1),
            title: row.get(2),
            description: row.get(3),
            start_time: row.get(4),
            active: row.get(5),
            track_points,
        })
    }
}

impl TrackSession {
    pub fn new(session_id: i64, trip_id: i64, title: String, description: String, timestamp: DateTime<Utc>, 
               active: bool, track_points: Vec<TrackPoint>) -> Self {
        Self {
            session_id,
            trip_id,
            title,
            description,
            start_time: timestamp,
            active,
            track_points,
        }
    }

    pub fn get_track_points_blob(&self) -> Vec<u8> {
        //TrackPoint::serialize_many(&self.track_points)
        todo!()
    }
}