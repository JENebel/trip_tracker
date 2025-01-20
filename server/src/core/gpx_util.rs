use core::time;
use std::str::FromStr;

use chrono::DateTime;
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession};

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

    TrackSession::new(-1, 0, time, false, track_points)
}

#[test]
fn test() {
    read_gpx("../test_data/syddjurs.gpx");
}