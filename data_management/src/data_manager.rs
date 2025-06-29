use std::{net::IpAddr, path::PathBuf};

use chrono::{DateTime, Utc};
use trip_tracker_lib::{track_point::TrackPoint, track_session::{SessionUpdate, TrackSession}, traffic::Visit, trip::Trip};

use crate::{buffer::buffer_manager::BufferManager, database::db::TripDatabase, geonames::CountryLookup, DataManagerError, DATA_DIR};

pub struct DataManager {
    pub(crate) database: TripDatabase,
    pub(crate) buffer_manager: BufferManager,
    country_lookup: CountryLookup,
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
        let country_lookup = CountryLookup::new();

        Ok(DataManager {
            database,
            buffer_manager,
            country_lookup,
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
        if session.active {
            // read buffer
            let buffered_points = self.buffer_manager.read_all_track_points(session_id).await?;
            session.track_points = buffered_points;
            Ok(session)
        } else {
            // read from database
            Ok(session)
        }
    }

    pub async fn get_session_update(&self, session_id: i64, timestamp: DateTime<Utc>) -> Result<SessionUpdate, DataManagerError> {
        let session = self.get_session(session_id).await?;

        let mut misssing_points;
        if session.active {
            // read buffer
            misssing_points = self.buffer_manager.read_track_points_since(session_id, timestamp).await?;
        } else {
            // read from database
            misssing_points = self.database.get_session(session_id).await?.track_points.iter().cloned().skip_while(|p| p.timestamp <= timestamp).collect();
        }

        misssing_points = misssing_points.into_iter().step_by(6).collect();

        Ok(SessionUpdate {
            session_id,
            title: session.title,
            description: session.description,
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

    pub async fn append_gps_points(&self, session_id: i64, points: &[TrackPoint]) -> Result<(), DataManagerError> {
        let session = self.database.get_session(session_id).await?;
        let trip = self.database.get_trip(session.trip_id).await?;
        let mut countries = trip.country_list.clone();
        let mut prev_country = None;
        let mut added = false;
        for point in points {
            let country = self.country_lookup.get_country(point.latitude, point.longitude, prev_country.clone());
            if let Some(country) = &country {
                if !countries.contains(&country) {
                    countries.push(country.clone());
                    added = true;
                }
            }
            prev_country = country;
        }

        if added {
            self.database.set_trip_countries(session.trip_id, countries).await?;
        }

        if session.active {
            self.buffer_manager.append_track_points(session_id, points).await
        } else {
            // If session is not active, append to database directly
            self.database.append_track_points(session_id, points).await
        }
    }

    pub async fn get_nonhidden_trip_session_ids(&self, trip_id: i64) -> Result<Vec<i64>, DataManagerError> {
        self.database.get_nonhidden_trip_session_ids(trip_id).await
    }

    pub async fn record_visit(&self, ip: IpAddr) -> Result<(), DataManagerError> {
        let visit = Visit {
            ip: ip.to_string(),
            timestamp: chrono::Utc::now(),
        };
        self.database.insert_visit(visit).await
    }
}

#[tokio::test]
async fn init_trip() {
    let dm = DataManager::start().await.unwrap();
    let _ = dm.register_new_trip("Tour de Lada 2025".into(), "Silas og Joachim kører en Lada fra Armenien til Danmark.\n Lykkes det at finde en Lada i god stand? Får vi lov til at køre igennem Tyrkiet? Får vi den ind i Danmark? Følg med og find ud af det!".into(), chrono::Utc::now()).await.unwrap();
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