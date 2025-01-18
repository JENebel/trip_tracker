use chrono::{DateTime, Utc};
use geo_types::Coord;
use serde::{Deserialize, Serialize};

use super::GpsPoint;

#[derive(Serialize, Deserialize)]
pub struct TrackPoint {
    pub position: Coord,
    pub timestamp: DateTime<Utc>,
}

impl TrackPoint {
    pub fn new(position: Coord, timestamp: DateTime<Utc>) -> Self {
        Self {
            position,
            timestamp,
        }
    }

    pub fn to_proto(&self) -> GpsPoint {
        GpsPoint { 
            latitude: self.position.x, 
            longitude: self.position.y, 
            timestamp: self.timestamp.timestamp_millis()
        }
    }
}

impl TryFrom<&[u8]> for TrackPoint {
    type Error = &'static str;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        bincode::deserialize_from(value).map_err(|_| "Failed to deserialize TrackPoint")
    }
}