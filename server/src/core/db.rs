use std::{path::PathBuf, time::SystemTime};

use sqlx::{query, query_as, sqlite::SqliteConnectOptions, Database, Executor, Pool, Sqlite, SqlitePool, Row};

use crate::data::{track_session::TrackSession, trip::Trip, TrackPoint};

const FILENAME: &str = "database.db";

#[derive(Clone)]
pub struct TripDatabase<T: Database> {
    pool: Pool<T>,
}

impl TripDatabase<Sqlite> {
    pub async fn connect() -> Self {
        let root: PathBuf = project_root::get_project_root().unwrap();
        let options = SqliteConnectOptions::new()
            .filename(root.join("server").join(FILENAME))
            .foreign_keys(true)
            .create_if_missing(true);
        
        let pool = SqlitePool::connect_with(options).await.unwrap();

        let db = Self {
            pool
        };

        db.init().await;

        db
    }

    pub async fn init(&self) {
        self.pool.execute("
            CREATE TABLE IF NOT EXISTS Trips (
                trip_id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                active BOOLEAN NOT NULL,
                start_time INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS Users (
                user_id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_name TEXT NOT NULL,
                join_time INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS TrackSessions (
                session_id INTEGER PRIMARY KEY AUTOINCREMENT,
                trip_id INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                active BOOLEAN NOT NULL,
                track_points BLOB NOT NULL,

                FOREIGN KEY(trip_id) REFERENCES Trips(trip_id) ON DELETE CASCADE
            )").await.unwrap();
    }

    pub async fn insert_track_session(&self, track_session: TrackSession) -> i64 {
        query_as::<_, (i64,)>("INSERT INTO TrackSessions (session_id, trip_id, timestamp, active, track_points) VALUES (NULL, ?1, ?2, ?3, ?4) RETURNING session_id")
            .bind(track_session.trip_id)
            .bind(track_session.timestamp)
            .bind(track_session.active)
            .bind(track_session.get_track_points_blob())
            .fetch_one(&self.pool).await.unwrap().0
    }

    pub async fn insert_trip(&self, trip: Trip) -> i64 {
        query_as::<_, (i64,)>("INSERT INTO Trips (trip_id, user_id, name, active, start_time) VALUES (NULL, ?1, ?2, ?3, ?4) RETURNING trip_id")
            .bind(trip.user_id)
            .bind(trip.name)
            .bind(trip.active)
            .bind(trip.start_time)
            .fetch_one(&self.pool).await.unwrap().0
    }

    pub async fn get_trips(&self) -> Vec<Trip> {
        query_as::<_, Trip>("SELECT * FROM Trips")
            .fetch_all(&self.pool).await.unwrap()
    }

    pub async fn get_trip(&self, trip_id: i64) -> Trip {
        query_as::<_, Trip>("SELECT * FROM Trips WHERE trip_id = ?1")
            .bind(trip_id)
            .fetch_one(&self.pool).await.unwrap()
    }

    pub async fn get_trip_sessions(&self, trip_id: i64) -> Vec<TrackSession> {
        query("SELECT * FROM TrackSessions WHERE trip_id = ?1")
            .bind(trip_id)
            .fetch_all(&self.pool).await.unwrap()
            .into_iter()
            .map(|row| TrackSession {
                session_id: row.get(0),
                trip_id: row.get(1),
                timestamp: row.get(2),
                active: row.get(3),
                track_points: bincode::deserialize(&row.get::<Vec<u8>, _>(4)).unwrap(),
            })
            .collect()
    }
}

#[tokio::test]
async fn test() {
    let db = TripDatabase::connect().await;

    let trip = Trip {
        trip_id: 0,
        user_id: 0,
        name: "Test".to_string(),
        active: true,
        start_time: SystemTime::now().into(),
    };

    let trip_id = db.insert_trip(trip).await;

    println!("{}", trip_id);

    /*
        Produce 10k points between 40.122151, 44.658078 and 56.158405, 10.206034
    */

    let track_points: Vec<TrackPoint> = (0..10_000).map(|i| {
        let x = 40.122151 + (56.158405 - 40.122151) * (i as f64 / 10_000.);
        let y = 44.658078 + (10.206034 - 44.658078) * (i as f64 / 10_000.);

        TrackPoint::new(geo_types::coord!{x: x, y: y}, SystemTime::now().into())
    }).collect();

    let track_session = TrackSession {
        session_id: 0,
        trip_id,
        timestamp: SystemTime::now().into(),
        active: true,
        track_points,
    };

    db.insert_track_session(track_session).await;
}