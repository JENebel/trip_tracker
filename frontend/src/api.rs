use reqwasm::http::Request;
use trip_tracker_lib::trip::Trip;

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
pub async fn get_default_trip() -> Result<Trip, ()> {
    let Ok(trip_ids) = make_request::<Vec<i64>>("/trip_ids").await else {
        return Err(());
    };

    if let Some(trip_id) = trip_ids.iter().max() {
        if let Ok(trip) = get_trip(*trip_id).await {
            return Ok(trip);
        }
    }

    return Err(());
}

pub async fn get_trip(trip_id: i64) -> Result<Trip, ()> {
    make_request(&format!("/trip/{trip_id}")).await
}