use std::{fs::File, io::BufWriter, str::FromStr, time::SystemTime};

use chrono::DateTime;
use geo::Point;
use gpx::{GpxVersion, Time, Track, TrackSegment, Waypoint};
use time::OffsetDateTime;
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession};

use crate::{DataManager, DataManagerError};

impl DataManager {
    pub async fn add_gpx_standalone(&self, path: &str) -> Result<(i64, i64), DataManagerError> {
        let track_session = crate::gpx_util::read_gpx(path);
        let trip = self.register_new_trip(track_session.title.clone(), track_session.description.clone(), track_session.start_time).await?;
        let session_id = self.register_new_session(trip.trip_id, track_session.title, track_session.description).await?.session_id;
        self.append_gps_points(session_id, &track_session.track_points).await?;
        Ok((trip.trip_id, session_id))
    }

    pub async fn add_gpx_to_trip(&self, path: &str, trip_id: i64, title: Option<&str>) -> Result<i64, DataManagerError> {
        let track_session = crate::gpx_util::read_gpx(path);
        let session_id = self.register_new_session(trip_id, title.unwrap_or(track_session.title.as_str()).into(), String::new()).await?.session_id;
        self.append_gps_points(session_id, &track_session.track_points).await?;
        Ok(session_id)
    }

    pub async fn export_gpx(self, session_id: i64) {
        let mut gpx = gpx::Gpx::default();
        gpx.version = GpxVersion::Gpx11;
    
        let session = self.get_session(session_id).await.unwrap();
    
        let start_time: SystemTime = session.start_time.into();
        let start_time: OffsetDateTime = start_time.into();
        gpx.metadata = Some(gpx::Metadata {
            name: Some(session.title.clone()),
            time: Some(Time::from(start_time)),
            ..Default::default()
        });
    
        let mut track = Track::new();
        let mut segment = TrackSegment::new();
        
        session.track_points.iter().for_each(|p| {
            let mut wp = Waypoint::new(Point::new(p.longitude, p.latitude));
            let time: SystemTime = p.timestamp.into();
            let time: OffsetDateTime = time.into();
            wp.time = Some(Time::from(time));
            segment.points.push(wp);
        });
    
        track.segments.push(segment);
        gpx.tracks.push(track);
    
        // Create file at path
        let gpx_file = File::create(format!("../data/gpx/{}.gpx", session.title)).unwrap();
        let buf = BufWriter::new(gpx_file);
    
        // Write to file
        gpx::write(&gpx, buf).unwrap();
    }
}

pub fn read_gpx(filename: &str) -> TrackSession {
    let file_path = project_root::get_project_root().unwrap().join("data").join("gpx").join(filename);
    let file = std::fs::File::open(file_path).unwrap();
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
                let track_point = if let Some(time) = point.time {
                    TrackPoint::new(
                        DateTime::from_str(&time.format().unwrap()).unwrap(),
                        point.point().0.x,
                        point.point().0.y,
                        0.,
                        0.,
                        true,
                    )
                } else {
                    TrackPoint::new(
                        time,
                        point.point().0.x,
                        point.point().0.y,
                        0.,
                        0.,
                        true,
                    )
                };
                track_points.push(track_point);
            }
        }
    }

    TrackSession::new(-1, 0, title, "".into(), time, false, track_points, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs;
    //use std::path::PathBuf;
    
    // Lada trip demo
    #[tokio::test]
    async fn add_lada_demo() {
        //let root: PathBuf = project_root::get_project_root().unwrap();
        //let _ = fs::remove_file(root.join("data/database.db")).await;

        // Dynamically add all gpx files in the demo folder to the database in sorted order
        let data_manager = DataManager::start().await.unwrap();

        let trip_id = data_manager.register_new_trip("Lada trip demo".into(), 
                                    "Demo of the Trip Tracker site for UI development".into(), 
                                    DateTime::parse_from_str("2025 May 22 12:09:14.274 +0000", "%Y %b %d %H:%M:%S%.3f %z").unwrap().into())
                    .await.unwrap().trip_id;

        let mut path_stream = fs::read_dir("../data/gpx/demo").await.unwrap();

        let mut paths = Vec::new();
        while let Some(entry) = path_stream.next_entry().await.unwrap() {
            paths.push(entry.path().file_name().unwrap().to_str().unwrap().to_string());
        }

        paths.sort();

        for path in paths.iter().filter(|p| *p != "live.gpx") {
            println!("{}", path);
            data_manager.add_gpx_to_trip(&format!("demo/{}", path), trip_id, Some(path.split_once(".").unwrap().0)).await.unwrap();
        }

        let session = data_manager.register_new_live_session(trip_id, "Live".into(), "description".into()).await.unwrap();
        let trip = read_gpx("demo/live.gpx");
        data_manager.append_gps_points(session.session_id, &trip.track_points).await.unwrap();
    }

    // Mols bjerge
    #[tokio::test]
    async fn add_mols_trip() {
        let data_manager = DataManager::start().await.unwrap();
        let (trip_id, _) = data_manager.add_gpx_standalone("mols/etape1.gpx").await.unwrap();
        data_manager.add_gpx_to_trip("mols/etape2.gpx", trip_id, None).await.unwrap();
        data_manager.add_gpx_to_trip("mols/etape3.gpx", trip_id, None).await.unwrap();
    }

    // Misc
    #[tokio::test]
    async fn add_gpx() {
        let data_manager = DataManager::start().await.unwrap();
        let (trip_id, _) = data_manager.add_gpx_standalone("Yerevan_i_sol.gpx").await.unwrap();

        println!("created trip with id: {trip_id}")
    }

    // Misc
    #[tokio::test]
    async fn add_error_gpx() {
        let data_manager = DataManager::start().await.unwrap();
        let (trip_id, _) = data_manager.add_gpx_standalone("errors.gpx").await.unwrap();

        println!("created trip with id: {trip_id}")
    }
}