use gloo_console::info;
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession};

pub fn haversine_distance(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    const R: f64 = 6372.8; // Radius of the earth in km

    let d_lat = (p2.0 - p1.0).to_radians();
    let d_lon = (p2.1 - p1.1).to_radians();
    let lat1 = p1.0.to_radians();
    let lat2 = p2.0.to_radians();

    let a = f64::sin(d_lat / 2.).powi(2)
        + f64::cos(lat1) * f64::cos(lat2) * f64::sin(d_lon / 2.).powi(2);
    let c = 2. * f64::asin(f64::sqrt(a));

    R * c
}

pub fn filter_anomalies(mut session: TrackSession) -> TrackSession {
    let mut filtered_points = Vec::new();
    // Filter out points that are very far from its neighbors
    for i in 1..session.track_points.len() - 1 {
        let prev_point = &session.track_points[i - 1];
        let curr_point = &session.track_points[i];
        let next_point = &session.track_points[i + 1];

        // Calculate the distance between the two points
        let dist_to_prev = haversine_distance((prev_point.latitude, prev_point.longitude), (curr_point.latitude, curr_point.longitude));
        let dist_to_next = haversine_distance((curr_point.latitude, curr_point.longitude), (next_point.latitude, next_point.longitude));
        let max_dist = dist_to_prev.max(dist_to_next);
        let dist_between_neighbors = haversine_distance((prev_point.latitude, prev_point.longitude), (next_point.latitude, next_point.longitude));
        //info!(format!("Min dist: {}, distance between neighbors: {}", dist_to_prev, dist_between_neighbors));

        // If the distance is too large, skip this point
        if max_dist > dist_between_neighbors * 5.0 {
            continue;
        }

        filtered_points.push(curr_point.clone());
    }
    
    info!(format!("Filtered away {}", session.track_points.len() - filtered_points.len()));

    session.track_points = filtered_points;

    info!(format!("{:?}", session.track_points));
    
    session
}