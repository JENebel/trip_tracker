use trip_tracker_lib::{track_session::TrackSession, trip::Trip};

// To be stored in the root and propagated to panel and map
#[derive(Debug, Clone, PartialEq)]
pub struct TripData {
    pub trip: Trip,
    pub sessions: Vec<TrackSession>,
}