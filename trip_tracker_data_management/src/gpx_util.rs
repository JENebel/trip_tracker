use std::str::FromStr;

use chrono::DateTime;
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession};

use crate::{DataManager, DataManagerError};

impl DataManager {
    pub async fn add_gpx_standalone(&self, path: &str) -> Result<(), DataManagerError> {
        let track_session = crate::gpx_util::read_gpx(path);
        let trip = self.register_new_trip(track_session.title.clone(), track_session.description.clone(), track_session.timestamp).await?;
        self.register_new_session(trip.trip_id, track_session.title, track_session.description).await?.session_id;
        self.set_session_track_points(track_session.session_id, track_session.track_points).await?;
        Ok(())
    }

    pub async fn add_gpx_to_trip(&self, path: &str, trip_id: i64) {
        let track_session = crate::gpx_util::read_gpx(path);
        self.register_new_session(trip_id, track_session.title.clone(), track_session.description.clone()).await.unwrap();
        self.set_session_track_points(track_session.session_id, track_session.track_points).await.unwrap();
    }
}

pub fn read_gpx(path: &str) -> TrackSession {
    let file = std::fs::File::open(path).unwrap();
    let reader = std::io::BufReader::new(file);
    let gpx = gpx::read(reader).unwrap();
    
    let mut time = DateTime::from_timestamp(0, 0).unwrap();

    let mut title = "Unnamed".to_string();
    if let Some(meta) = gpx.metadata {
        if let Some(name) = meta.name {
            title = name;
        }

        if let Some(t) = meta.time {
            time = DateTime::from_str(&t.format().unwrap()).unwrap();
        }
    }

    let mut track_points: Vec<TrackPoint> = Vec::new();
    for track in gpx.tracks {
        for segment in track.segments {
            for point in segment.points {
                let time = point.time.unwrap();
                let track_point = TrackPoint::new(
                    point.point(),
                    DateTime::from_str(&time.format().unwrap()).unwrap(),
                );
                track_points.push(track_point);
            }
        }
    }

    TrackSession::new(-1, 0, title, "".into(), time, false, track_points)
}

#[tokio::test]
async fn test() {
    DataManager::start().await.unwrap().add_gpx_standalone("../test_data/syddjurs.gpx").await.unwrap();
}