use std::{io::Read, path::PathBuf, str::FromStr};

use chrono::DateTime;
use tokio::fs::{self, File};
use trip_tracker_lib::{track_point::{parse_tsf, TrackPoint}, track_session::TrackSession};

use crate::{DataManager, DataManagerError};

impl DataManager {
    pub async fn add_tsf_standalone(&self, path: &str) -> Result<(i64, i64), DataManagerError> {
        let track_session = crate::tsf_util::read_tsf(path);
        let trip = self.register_new_trip(track_session.title.clone(), track_session.description.clone(), track_session.start_time).await?;
        let session_id = self.register_new_session(trip.trip_id, track_session.title, track_session.description).await?.session_id;
        self.set_session_track_points(session_id, track_session.track_points).await?;
        Ok((trip.trip_id, session_id))
    }

    pub async fn add_tsf_to_trip(&self, path: &str, trip_id: i64, title: Option<&str>) -> Result<i64, DataManagerError> {
        let track_session = crate::tsf_util::read_tsf(path);
        let session_id = self.register_new_session(trip_id, title.unwrap_or(track_session.title.as_str()).into(), track_session.description).await?.session_id;
        self.set_session_track_points(session_id, track_session.track_points).await?;
        Ok(session_id)
    }
}

pub fn read_tsf(filename: &str) -> TrackSession {
    let file_path = project_root::get_project_root().unwrap().join("data").join("tsf").join(filename);
    let mut file = std::fs::File::open(file_path).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();

    let (track_points, start_time) = parse_tsf(&bytes).unwrap();

    TrackSession::new(-1, 0, "TSF session".into(), "".into(), start_time, false, track_points)
}

// Misc
#[tokio::test]
async fn add_tsf() {
    let data_manager = DataManager::start().await.unwrap();

    let (_trip_id, _) = data_manager.add_tsf_standalone("test.tsf").await.unwrap();
}