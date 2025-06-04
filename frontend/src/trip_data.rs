use trip_tracker_lib::{track_session::TrackSession, trip::Trip};

#[derive(Debug, Clone, PartialEq)]
pub struct SessionData {
    pub session: TrackSession,
    pub distance: f64,
}

impl SessionData {
    pub fn from_session(session: TrackSession) -> Self {
        let distance = session.distance();
        Self {
            session,
            distance
        }
    }
}

// To be stored in the root and propagated to panel and map
#[derive(Debug, Clone, PartialEq)]
pub struct TripData {
    pub trip: Trip,
    pub sessions: Vec<SessionData>,
}