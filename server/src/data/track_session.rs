use chrono::{DateTime, Utc};
use sqlx::FromRow;

use super::{track_point::TrackPoint, GpsTrack};

#[derive(FromRow)]
pub struct TrackSession {
    pub session_id: i64,
    pub trip_id: i64,
    pub timestamp: DateTime<Utc>,
    pub active: bool,

    #[sqlx(try_from = "Vec<TrackPoint>")]
    pub track_points: Vec<TrackPoint>,
}

impl TrackSession {
    pub fn get_track_points_blob(&self) -> Vec<u8> {
        bincode::serialize(&self.track_points).unwrap()
    }

    pub fn to_proto(&self) -> GpsTrack {
        GpsTrack {
            name: self.session_id.to_string(),
            start_time: self.timestamp.timestamp_millis(),
            points: self.track_points.iter().map(|tp| tp.to_proto()).collect(),
        }
    }
}