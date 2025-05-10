use leaflet::{Layer, Polyline};
use trip_tracker_lib::{track_session::{SessionUpdate, TrackSession}, trip::Trip};

pub enum TripMessage {
    TripLoaded(TripData),
    TripUpdated(Trip),
    SessionUpdated(SessionUpdate)
}

// To be stored in the root and propagated to panel and map
#[derive(Debug, Clone)]
pub struct TripData {
    pub trip: Trip,
    pub inactive_sessions: Vec<TrackSession>,
    pub active_sessions: Vec<TrackSession>,
}

impl PartialEq for TripData {
    fn eq(&self, other: &Self) -> bool {
        self.trip == other.trip && 
        self.inactive_sessions.len() == other.inactive_sessions.len() &&
        self.active_sessions.len() == other.inactive_sessions.len()
    }
}