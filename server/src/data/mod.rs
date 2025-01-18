pub mod track_point;
pub mod track_session;
pub mod trip;
pub mod user;

include!(concat!(env!("OUT_DIR"), "/gps_track.rs"));

pub use track_point::TrackPoint;

#[test]
fn test() {

}