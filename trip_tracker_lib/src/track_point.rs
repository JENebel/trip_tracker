use chrono::{DateTime, Utc};
use geo_types::Point;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TrackPoint {
    pub position: Point,
    pub timestamp: DateTime<Utc>,
}

impl TrackPoint {
    pub fn new(position: Point, timestamp: DateTime<Utc>) -> Self {
        Self {
            position,
            timestamp,
        }
    }
}

impl TryFrom<&[u8]> for TrackPoint {
    type Error = &'static str;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize_from(value).map_err(|_| "Failed to deserialize TrackPoint")
    }
}