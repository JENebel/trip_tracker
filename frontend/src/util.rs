use gloo_console::info;
use trip_tracker_lib::{haversine_distance, track_point::TrackPoint, track_session::TrackSession};

pub fn filter_anomalies(mut session: TrackSession) -> TrackSession {
    return session;
    
    let mut filtered_points = Vec::new();
    // Filter out points that are very far from its neighbors, and points that go "back" in time.
 
    if session.track_points.len() < 3 {
        info!("Not enough points to filter anomalies, returning original session. for session: {}", session.session_id);
        return session;
    }

    //session.track_points.sort_by_key(|p| p.timestamp);

    let mut prev_point = &session.track_points[0];
    for i in 1..session.track_points.len() - 1 {
        let curr_point = &session.track_points[i];
        let next_point = &session.track_points[i + 1];

        // Calculate the distance between the two points
        let dist_to_prev = haversine_distance((prev_point.latitude, prev_point.longitude), (curr_point.latitude, curr_point.longitude));
        let dist_to_next = haversine_distance((curr_point.latitude, curr_point.longitude), (next_point.latitude, next_point.longitude));
        let neighbor_dist = haversine_distance((prev_point.latitude, prev_point.longitude), (next_point.latitude, next_point.longitude));

        if dist_to_prev + dist_to_next > neighbor_dist * 5. {
            continue;
        }

        if dist_to_prev > 5.0 {
            continue;           
        }

        if filtered_points.iter().any(|p: &TrackPoint| p.latitude == curr_point.latitude && p.longitude == curr_point.longitude) {
            continue;
        }

        filtered_points.push(curr_point.clone());
        prev_point = curr_point;
    }
    
    info!(format!("Filtered away {}", session.track_points.len() - filtered_points.len()));

    session.track_points = filtered_points;

    session
}