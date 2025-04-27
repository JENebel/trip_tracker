use std::path::PathBuf;

use chrono::{DateTime, Utc};
use trip_tracker_lib::{track_point::TrackPoint, track_session::{SessionUpdate, TrackSession}, trip::Trip};

use crate::{buffer::buffer_manager::{self, BufferManager}, database::db::TripDatabase, DataManagerError, DATA_DIR};

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
        //let mut api_token = rand::
        let mut api_token = [0u8; 32];
        rand::fill(&mut api_token);
        self.database.insert_trip(title, description, start_time, hex::encode(api_token)).await
    }

    pub async fn register_new_session(&self, trip_id: i64, title: String, description: String) -> Result<TrackSession, DataManagerError> {
        self.database.insert_track_session(trip_id, title, description, chrono::Utc::now(), false).await
    }

    pub async fn register_new_live_session(&self, trip_id: i64, title: String, description: String) -> Result<TrackSession, DataManagerError> {
        let session = self.database.insert_track_session(trip_id, title, description, chrono::Utc::now(), true).await?;
        self.buffer_manager.start_session(&session).await?;
        Ok(session)
    }

    pub async fn set_session_track_points(&self, session_id: i64, track_points: Vec<TrackPoint>) -> Result<(), DataManagerError> {
        self.database.set_session_track_points(session_id, track_points).await
    }

    pub async fn get_trips(&self) -> Result<Vec<Trip>, DataManagerError> {
        self.database.get_trips().await
    }

    pub async fn get_trip(&self, trip_id: i64) -> Result<Trip, DataManagerError> {
        self.database.get_trip(trip_id).await
    }

    pub async fn get_trip_sessions(&self, trip_id: i64) -> Result<Vec<TrackSession>, DataManagerError> {
        let mut sessions = self.database.get_trip_sessions(trip_id).await.unwrap();

        for session in sessions.iter_mut().filter(|session| session.active) {
            let buffered_points = self.buffer_manager.read_all_track_points(session.session_id).await?;
            session.track_points = buffered_points;
        }

        Ok(sessions)
    }

    pub async fn get_session(&self, session_id: i64) -> Result<TrackSession, DataManagerError> {
        let mut session = self.database.get_session(session_id).await?;
        let buffered_points = self.buffer_manager.read_all_track_points(session_id).await?;
        session.track_points = buffered_points;
        Ok(session)
    }

    pub async fn get_session_update(&self, session_id: i64, current_points: usize) -> Result<SessionUpdate, DataManagerError> {
        let session = self.get_session(session_id).await?;

        let misssing_points;
        if session.active {
            // read buffer
            misssing_points = self.buffer_manager.read_track_points_since(session_id, current_points).await?;
        } else {
            // read from database
            misssing_points = self.database.get_session(session_id).await?.track_points[current_points..].to_vec();
        }

        Ok(SessionUpdate {
            new_track_points: misssing_points,
            still_active: session.active,
        })
    }

    pub async fn end_session(&self, session_id: i64) -> Result<(), DataManagerError> {
        let points = self.buffer_manager.close_session(session_id).await?;
        self.database.set_session_track_points(session_id, points).await?;
        self.database.set_session_active(session_id, false).await?;
        
        Ok(())
    }

    pub async fn append_gps_point(&self, session_id: i64, points: &[TrackPoint]) -> Result<(), DataManagerError> {
        self.buffer_manager.append_track_points(session_id, points).await
    }

    pub async fn get_trip_session_ids(&self, trip_id: i64) -> Result<Vec<i64>, DataManagerError> {
        self.database.get_trip_session_ids(trip_id).await
    }
}

#[tokio::test]
async fn init_trip() {
    let dm = DataManager::start().await.unwrap();
    let trip = dm.register_new_trip("Test Trip".into(), "".into(), chrono::Utc::now()).await.unwrap();
    println!("{:?}", trip);
}

#[tokio::test]
async fn clear_sessions() {
    let dm = DataManager::start().await.unwrap();
    let trips = dm.get_trips().await.unwrap();
    for trip in trips {
        let sessions = dm.get_trip_sessions(trip.trip_id).await.unwrap();
        for session in sessions {
            dm.end_session(session.session_id).await.unwrap();
        }
    }
}