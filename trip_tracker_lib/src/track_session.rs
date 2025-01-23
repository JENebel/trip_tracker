use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::track_point::TrackPoint;

#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
#[derive(Serialize, Deserialize)]
pub struct TrackSession {
    pub session_id: i64,
    pub trip_id: i64,
    pub timestamp: DateTime<Utc>,
    pub title: String,
    pub description: String,
    pub active: bool,

    #[cfg_attr(feature = "sqlx", sqlx(try_from = "Vec<TrackPoint>"))]
    pub track_points: Vec<TrackPoint>,
}

impl TrackSession {
    pub fn new(session_id: i64, trip_id: i64, title: String, description: String, timestamp: DateTime<Utc>, 
               active: bool, track_points: Vec<TrackPoint>) -> Self {
        Self {
            session_id,
            trip_id,
            title,
            description,
            timestamp,
            active,
            track_points,
        }
    }

    pub fn get_track_points_blob(&self) -> Vec<u8> {
        bincode::serialize(&self.track_points).unwrap()
    }
}