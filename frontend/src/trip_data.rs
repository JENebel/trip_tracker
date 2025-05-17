use trip_tracker_lib::{track_session::TrackSession, trip::Trip};

use crate::util;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionData {
    pub session: TrackSession,
    pub distance: f64,
}

impl SessionData {
    pub fn from_session(session: TrackSession) -> Self {
        let distance = calc_distance(&session);
        Self {
            session,
            distance
        }
    }
}

pub fn calc_distance(session: &TrackSession) -> f64 {
    let mut distance = 0.;
    for i in 1..session.track_points.len() {
        let prev = &session.track_points[i - 1];
        let curr = &session.track_points[i];
        distance += util::haversine_distance((prev.latitude, prev.longitude), (curr.latitude, curr.longitude));
    }
    distance
}

// To be stored in the root and propagated to panel and map
#[derive(Debug, Clone, PartialEq)]
pub struct TripData {
    pub trip: Trip,
    pub sessions: Vec<SessionData>,
}