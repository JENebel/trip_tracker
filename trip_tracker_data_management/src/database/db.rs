use std::{collections::HashMap, path::PathBuf};

use chrono::{DateTime, Utc};
use const_format::concatcp;
use sqlx::{query, query_as, sqlite::SqliteConnectOptions, Executor, Pool, Sqlite, SqlitePool, Row};
use trip_tracker_lib::{track_point::{write_tsf, TrackPoint}, track_session::TrackSession, traffic::{IpInfo, SiteTrafficData, Visit}, trip::Trip};

use crate::{DataManagerError, DATABASE_PATH};

use super::constants::*;

#[derive(Clone)]
pub struct TripDatabase {
    pool: Pool<Sqlite>,
}

impl TripDatabase {
    pub async fn connect() -> Result<Self, DataManagerError> {
        let root: PathBuf = project_root::get_project_root().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(root.join(DATABASE_PATH))
            .foreign_keys(true)
            .create_if_missing(true);
        
        let pool = SqlitePool::connect_with(options).await.map_err(|_| DataManagerError::Database("Failed to connect to database".to_string()))?;

        let db = Self {
            pool
        };

        db.init().await;

        Ok(db)
    }

    pub async fn init(&self) {
        self.pool.execute(concatcp!("
            CREATE TABLE IF NOT EXISTS ", TRIPS_TABLE_NAME, "(", 
                TRIP_ID,      " INTEGER PRIMARY KEY AUTOINCREMENT,",
                TIMESTAMP,    " TIMESTAMP NOT NULL,",
                TITLE,        " TEXT NOT NULL,", 
                DESCRIPTION,  " TEXT,", 
                API_TOKEN,    " TEXT NOT NULL,",
                COUNTRY_LIST, " BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ", TRACK_SESSIONS_TABLE_NAME, "(",
                SESSION_ID,   " INTEGER PRIMARY KEY AUTOINCREMENT,",
                TRIP_ID,      " INTEGER NOT NULL,",
                TITLE,        " TEXT NOT NULL,",
                DESCRIPTION,  " TEXT,",
                TIMESTAMP,    " TIMESTAMP NOT NULL,",
                ACTIVE,       " BOOLEAN NOT NULL,",
                TRACK_POINTS, " BLOB NOT NULL,
                FOREIGN KEY(", TRIP_ID, ") REFERENCES ", TRIPS_TABLE_NAME, "(", TRIP_ID, ") ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS ", VISIT_TABLE, "(",
                VISIT_ID,   " INTEGER PRIMARY KEY AUTOINCREMENT,",
                IP_ADDRESS, " TEXT PRIMARY KEY,",
                TIMESTAMP,  " TIMESTAMP NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ", IP_INFO_TABLE_NAME, "(",
                IP_ADDRESS, " TEXT PRIMARY KEY,",
                COUNTRY,    " TEXT NOT NULL,",
                LATITUDE,   " REAL NOT NULL,",
                LONGITUDE,  " REAL NOT NULL
            );

            ")).await.unwrap();
    }

    pub async fn insert_trip(&self, title: String, description: String, timestamp: DateTime<Utc>, api_token: String) -> Result<Trip, DataManagerError> {
        let id = query_as::<_, (i64,)>(concatcp!("
            INSERT INTO ", TRIPS_TABLE_NAME, "(", 
            TRIP_ID, ", ", TIMESTAMP, ", ", TITLE, ", ", DESCRIPTION, ", ", API_TOKEN, ", ", COUNTRY_LIST, ")
            VALUES (NULL, ?1, ?2, ?3, ?4, ?5) RETURNING ", TRIP_ID))
                .bind(timestamp)
                .bind(&title)
                .bind(&description)
                .bind(&api_token)
                .bind(Vec::new())
                .fetch_one(&self.pool).await
                .map_err(|_| DataManagerError::Database("Failed to insert trip".to_string()))
                .map(|row| row.0)?;

        Ok(Trip::new(id, title.clone(), description.clone(), timestamp, api_token.clone()))
    }

    pub async fn set_trip_title(&self, trip_id: i64, title: &String) -> Result<(), DataManagerError> {
        query(concatcp!("UPDATE ", TRIPS_TABLE_NAME, " SET ", TITLE, " = ?1, WHERE ", TRIP_ID, " = ?2"))
                .bind(title)
                .bind(trip_id)
                .execute(&self.pool).await
                .map_err(|_| DataManagerError::Database("Failed to update trip title".to_string()))
                .map(|_| ())
    }

    pub async fn set_trip_description(&self, trip_id: i64, description: &String) -> Result<(), DataManagerError> {
        query(concatcp!("UPDATE ", TRIPS_TABLE_NAME, " SET ", DESCRIPTION, " = ?1, WHERE ", TRIP_ID, " = ?2"))
                .bind(description)
                .bind(trip_id)
                .execute(&self.pool).await
                .map_err(|_| DataManagerError::Database("Failed to update trip description".to_string()))
                .map(|_| ())
    }

    pub async fn insert_track_session(&self, trip_id: i64, title: String, description: String, start_time: DateTime<Utc>, active: bool) -> Result<TrackSession, DataManagerError> {
        let session_id = query_as::<_, (i64,)>(concatcp!("
            INSERT INTO ", TRACK_SESSIONS_TABLE_NAME, 
            "(", SESSION_ID, ", ", TRIP_ID, ", ", TITLE, ", ", DESCRIPTION, ", ", TIMESTAMP, ", ", ACTIVE, ", ", TRACK_POINTS, ")
            VALUES (NULL, ?1, ?2, ?3, ?4, ?5, ?6) RETURNING ", SESSION_ID))
                .bind(trip_id)
                .bind(&title)
                .bind(&description)
                .bind(&start_time)
                .bind(active)
                .bind(Vec::new())
                .fetch_one(&self.pool).await
                .map_err(|_| DataManagerError::Database("Failed to insert track session".to_string()))
                .map(|row| row.0)?;

        Ok(TrackSession::new(session_id, trip_id, title, description, start_time, active, Vec::new()))
    }

    pub async fn get_session(&self, session_id: i64) -> Result<TrackSession, DataManagerError> {
        query_as::<_, TrackSession>(concatcp!("SELECT * FROM ", TRACK_SESSIONS_TABLE_NAME, " WHERE ", SESSION_ID, " = ?1"))
            .bind(session_id)
            .fetch_one(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to get session".to_string()))
            .map(|row| row)
    }

    pub async fn set_session_active(&self, session_id: i64, active: bool) -> Result<(), DataManagerError> {
        query(concatcp!("UPDATE ", TRACK_SESSIONS_TABLE_NAME, " SET ", ACTIVE, " = ?1 WHERE ", SESSION_ID, " = ?2"))
            .bind(active)
            .bind(session_id)
            .execute(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to set session active".to_string()))
            .map(|_| ())
    }

    pub async fn set_session_track_points(&self, session_id: i64, track_points: Vec<TrackPoint>) -> Result<(), DataManagerError> {
        let start_time = track_points.first().map(|point| point.timestamp).unwrap_or_else(|| Utc::now()); // If none, time will not be used, so it doesn't matter
        query(concatcp!("UPDATE ", TRACK_SESSIONS_TABLE_NAME, " SET ", TRACK_POINTS, " = ?1 WHERE ", SESSION_ID, " = ?2"))
            .bind(write_tsf(start_time, &track_points))
            .bind(session_id)
            .execute(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to set session track points".to_string()))
            .map(|_| ())
    }

    pub async fn append_track_points(&self, session_id: i64, track_points: &[TrackPoint]) -> Result<(), DataManagerError> {
        let session = self.get_session(session_id).await?;
        let mut all_track_points = session.track_points.clone();
        all_track_points.extend_from_slice(track_points);
        self.set_session_track_points(session_id, all_track_points).await
    }

    pub async fn get_trip(&self, trip_id: i64) -> Result<Trip, DataManagerError> {
        query_as::<_, Trip>(concatcp!("SELECT * FROM ", TRIPS_TABLE_NAME, " WHERE ", TRIP_ID, " = ?1"))
            .bind(trip_id)
            .fetch_one(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to get trip".to_string()))
            .map(|row| row)
    }

    pub async fn get_trips(&self) -> Result<Vec<Trip>, DataManagerError> {
        query(concatcp!("SELECT * FROM ", TRIPS_TABLE_NAME))
            .fetch_all(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to get trips".to_string()))
            .map(|rows| rows.into_iter()
                .map(|row| Trip {
                    trip_id: row.get(0),
                    timestamp: row.get(1),
                    title: row.get(2),
                    description: row.get(3),
                    api_token: row.get(4),
                    country_list: Vec::new(), // TODO: Get country codes
                }).collect()
            )
    }

    pub async fn get_trip_sessions(&self, trip_id: i64) -> Result<Vec<TrackSession>, DataManagerError> {
        query_as::<_, TrackSession>(concatcp!("SELECT * FROM ", TRACK_SESSIONS_TABLE_NAME, " WHERE ", TRIP_ID, " = ?1"))
            .bind(trip_id)
            .fetch_all(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to get session".to_string()))
    }

    pub async fn get_trip_session_ids(&self, trip_id: i64) -> Result<Vec<i64>, DataManagerError> {
        query(concatcp!("SELECT ", SESSION_ID, " FROM ", TRACK_SESSIONS_TABLE_NAME, " WHERE ", TRIP_ID, " = ?1"))
            .bind(trip_id)
            .fetch_all(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to get session".to_string()))
            .map(|rows| rows.into_iter()
                .map(|row| row.get(0))
                .collect()
            )
    }

    pub async fn set_trip_countries(&self, trip_id: i64, country_codes: Vec<String>) -> Result<(), DataManagerError> {
        query(concatcp!("UPDATE ", TRIPS_TABLE_NAME, " SET ", COUNTRY_LIST, " = ?1 WHERE ", TRIP_ID, " = ?2"))
            .bind(bincode::serialize(&country_codes).unwrap())
            .bind(trip_id)
            .execute(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to set trip countries".to_string()))
            .map(|_| ())
    }

    pub async fn get_site_traffic_info(&self) -> Result<SiteTrafficData, DataManagerError> {
        // Get all visits, but ignore id
        let visits = query(concatcp!("SELECT ", IP_ADDRESS, ", ", TIMESTAMP, " FROM ", VISIT_TABLE))
            .fetch_all(&self.pool).await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| Visit {
                        ip: row.get(1),
                        timestamp: row.get(2),
                    })
                    .collect::<Vec<Visit>>()
            })
            .map_err(|_| DataManagerError::Database("Failed to get site traffic data".to_string()))?;

        let mut visitor_infos = HashMap::new(); 

        // Get all IP info
        for visit in visits.iter() {
            let ip_info = match query_as::<_, IpInfo>(concatcp!("SELECT * FROM ", IP_INFO_TABLE_NAME, " WHERE ", IP_ADDRESS, " = ?1"))
                .bind(visit.ip.clone())
                .fetch_one(&self.pool).await {
                Ok(ip_info) => ip_info,
                Err(_) => {
                    // Get IP info from web
                    let Ok(ip_info) = get_ip_info(visit.ip.clone()).await else {
                        continue;
                    };

                    // Insert IP info into database
                    self.insert_ip_info(ip_info.clone())
                        .await
                        .map_err(|_| DataManagerError::Database("Failed to insert IP info".to_string()))?;

                    ip_info
                }
            };

            visitor_infos.insert(visit.ip.clone(), ip_info);
        }

        Ok(SiteTrafficData {
            visits,
            ip_info: visitor_infos,
        })
    }

    pub async fn insert_ip_info(&self, ip_info: IpInfo) -> Result<(), DataManagerError> {
        query(concatcp!("INSERT INTO ", IP_INFO_TABLE_NAME, "(", 
            IP_ADDRESS, ", ", COUNTRY, ", ", LATITUDE, ", ", LONGITUDE, ") VALUES (?1, ?2, ?3, ?4)"))
            .bind(ip_info.ip)
            .bind(&ip_info.country)
            .bind(ip_info.latitude)
            .bind(ip_info.longitude)
            .execute(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to insert IP info".to_string()))
            .map(|_| ())
    }

    pub async fn insert_visit(&self, visit: Visit) -> Result<(), DataManagerError> {
        query(concatcp!("INSERT INTO ", VISIT_TABLE, "(", 
            IP_ADDRESS, ", ", TIMESTAMP, ") VALUES (NULL, ?1, ?2)"))
            .bind(visit.ip)
            .bind(visit.timestamp)
            .execute(&self.pool).await
            .map_err(|_| DataManagerError::Database("Failed to record visit".to_string()))
            .map(|_| ())
    }
}

async fn get_ip_info(ip: String) -> Result<IpInfo, DataManagerError> {
    let response = reqwest::get(format!("http://ip-api.com/json/{}", ip))
        .await
        .map_err(|_| DataManagerError::Database("Failed to get IP info".to_string()))?;

    let response = response.text()
        .await
        .map_err(|_| DataManagerError::Database("Failed to get IP info".to_string()))?;

    let mut json = json::parse(&response).map_err(|_| DataManagerError::Database("Failed to parse IP info".to_string()))?;

    let country = json.remove("country");
    let Some(country) = country.as_str() else {
        return Err(DataManagerError::Database("Failed to get country code".to_string()));
    };

    let Some(latitude) = json.remove("lat").as_f64() else {
        return Err(DataManagerError::Database("Failed to get latitude".to_string()));
    };

    let Some(longitude) = json.remove("lon").as_f64() else {
        return Err(DataManagerError::Database("Failed to get longitude".to_string()));
    };

    Ok(IpInfo {
        ip,
        country: country.to_string(),
        latitude: latitude as f32,
        longitude: longitude as f32,
    })
}