use std::path::PathBuf;

use chrono::{DateTime, Utc};
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession, trip::Trip};

use crate::{buffer::buffer::BufferManager, database::db::TripDatabase, DataManagerError, DATA_DIR};

#[derive(Clone)]
pub struct DataManager {
    pub(crate) database: TripDatabase,
    pub(crate) buffer_manager: BufferManager,
}

/// The public interface for all trip tracker data management.
impl DataManager {
    pub async fn start() -> Result<Self, DataManagerError> {
        // Create data dir if it doesn't exist
        let root: PathBuf = project_root::get_project_root().unwrap();
        let data_dir = root.join(DATA_DIR);
        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir)
                .map_err(|_| DataManagerError::Database(format!("Failed to create data directory: {:?}", data_dir)))?;
        }

        let buffer_manager = BufferManager::start().await?;
        let database = TripDatabase::connect().await?;

        Ok(DataManager {
            database,
            buffer_manager,
        })
    }

    pub async fn register_new_trip(&self, title: String, description: String, start_time: DateTime<Utc>) -> Result<Trip, DataManagerError> {
        self.database.insert_trip(title, description, start_time, "".into()).await
    }

    /// Defaults to inactive, set active with set_session_active
    pub async fn register_new_session(&self, trip_id: i64, title: String, description: String) -> Result<TrackSession, DataManagerError> {
        self.database.insert_track_session(trip_id, title, description, chrono::Utc::now(), false).await
    }

    pub async fn set_session_active(&self, session_id: i64, active: bool) -> Result<(), DataManagerError> {
        self.database.set_session_active(session_id, active).await
    }

    pub async fn set_session_track_points(&self, session_id: i64, track_points: Vec<TrackPoint>) -> Result<(), DataManagerError> {
        self.database.set_session_track_points(session_id, track_points).await
    }

    pub async fn get_trips(&self) -> Result<Vec<Trip>, DataManagerError> {
        self.database.get_trips().await
    }

    pub async fn get_trip_sessions(&self, trip_id: i64) -> Result<Vec<TrackSession>, DataManagerError> {
        let mut sessions = self.database.get_trip_sessions(trip_id).await.unwrap();

        for session in sessions.iter_mut() {
            if session.active {
                let buffered_points = self.buffer_manager.read_buffer(session.session_id).await?;
                session.track_points = buffered_points;
            }
        }

        Ok(sessions)
    }

    pub async fn end_session(&self, session_id: i64) -> Result<(), DataManagerError> {
        let points = self.buffer_manager.close_session(session_id).await?;
        self.database.set_session_track_points(session_id, points).await?;
        self.database.set_session_active(session_id, false).await?;
        Ok(())
    }

    pub async fn append_gps_point(&self, session_id: i64, point: TrackPoint) -> Result<(), DataManagerError> {
        self.buffer_manager.append_track_point(session_id, point).await
    }
}

#[tokio::test]
async fn test() {
    DataManager::start().await.unwrap();
}