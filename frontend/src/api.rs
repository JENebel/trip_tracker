use gloo_net::http::Request;
use serde::de::DeserializeOwned;
use trip_tracker_lib::{track_session::{SessionUpdate, TrackSession}, trip::Trip};

pub async fn make_request<ReturnType>(path: &str) -> Result<ReturnType, ()>
where
    ReturnType: DeserializeOwned,
{
    let response = Request::get(path)
        .send()
        .await
        .map_err(|err| {
            web_sys::console::error_1(&format!("Request error: {:?}", err).into());
            ()
        })?;

    let bytes = response
        .binary()
        .await
        .map_err(|err| {
            web_sys::console::error_1(&format!("Binary read error: {:?}", err).into());
            ()
        })?;

    let result = bincode::deserialize::<ReturnType>(&bytes)
        .map_err(|err| {
            web_sys::console::error_1(&format!("Deserialization error: {:?}", err).into());
            ()
        })?;

    Ok(result)
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

pub async fn get_session_update(session_id: i64, timestamp: i64) -> Result<SessionUpdate, ()> {
    make_request(&format!("/session_update/{session_id}/{timestamp}")).await
}