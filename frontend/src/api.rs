use reqwasm::http::Request;
use trip_tracker_lib::{track_session::TrackSession, trip::Trip};

pub async fn make_request<ReturnType>(path: &str) -> Result<ReturnType, ()>
where
    ReturnType: serde::de::DeserializeOwned,
{
    let Ok(response) = Request::get(path).send().await else {
        return Err(());
    };

    let Ok(binary) = response.binary().await else {
        return Err(());
    };

    let Ok(result) = bincode::deserialize::<ReturnType>(&binary) else {
        return Err(());
    };

    return Ok(result);
}

// default to newest trip
pub async fn get_default_trip_id() -> Result<i64, ()> {
    let Ok(trip_ids) = make_request::<Vec<i64>>("/trip_ids").await else {
        return Err(());
    };

    if let Some(trip_id) = trip_ids.iter().max() {
        return Ok(*trip_id);
    }

    return Err(());
}

pub async fn get_trip(trip_id: i64) -> Result<Trip, ()> {
    make_request(&format!("/trip/{trip_id}")).await
}

pub async fn get_trip_session_ids(trip_id: i64) -> Result<Vec<i64>, ()> {
    make_request(&format!("/session_ids/{trip_id}")).await
}

pub async fn get_session(session_id: i64) -> Result<TrackSession, ()> {
    make_request(&format!("/session/{session_id}")).await
}