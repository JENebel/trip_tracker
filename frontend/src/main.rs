use gloo_console::{error, info};
use crate::components::{
    panel::Panel,
    map_component::{MapComponent, Point},
};
use reqwasm::http::Request;
use trip_tracker_lib::{track_session::TrackSession, trip::Trip};
use wasm_bindgen_futures::spawn_local;
use web_sys::js_sys;
use yew::prelude::*;
mod components;

enum Msg {
    //SetViewLocation(Point),
    SetTrip(Option<Trip>),
}

struct Model {
    selected_trip: Option<Trip>,
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link().clone();

        get_newest_trip(link.callback(Msg::SetTrip));

        Self { selected_trip: None }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            /*Msg::SetViewLocation(point) => {
                
            }*/
            Msg::SetTrip(trip) => {
                self.selected_trip = trip;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let cb = ctx.link().callback(Msg::SetTrip);
        let point = Point(56.175188, 10.196123);
        html! {
            <>
                <Panel select_trip={cb} />
                <MapComponent pos={point} />
            </>
        }
    }
}

fn get_newest_trip(callback: Callback<Option<Trip>>) {
    spawn_local(async move {
        let before = js_sys::Date::new_0().get_time();

        let Ok(response) = Request::get("/trips").send().await else {
            error!("Failed to fetch trips");
            callback.emit(None);
            return;
        };

        let Ok(binary) = response.binary().await else {
            error!("Response was not binary");
            return;
        };

        let Ok(trips) = bincode::deserialize::<Vec<Trip>>(&binary) else {
            error!("Response could not be deserialized");
            callback.emit(None);
            return;
        };

        let after = js_sys::Date::new_0().get_time();

        info!(format!("Fetched {} trips in {:?} ms", trips.len(), after - before));
        
        let Some(newest) = trips.into_iter().max_by_key(|trip| trip.timestamp) else {
            error!("No trips found");
            callback.emit(None);
            return;
        };

        callback.emit(Some(newest));
    });
}

fn fetch_sessions(callback: Callback<Vec<TrackSession>>, trip_id: i64) {
    spawn_local(async move {
        let before = js_sys::Date::new_0().get_time();
        
        let Ok(response) = Request::get(&format!("/sessions/{trip_id}")).send().await else {
            error!("Failed to fetch tracks");
            return;
        };

        let Ok(binary) = response.binary().await else {
            error!("Response was not binary");
            return;
        };

        let Ok(sessions) = bincode::deserialize::<Vec<TrackSession>>(&binary) else {
            error!("Response could not be deserialized");
            return;
        };

        let after = js_sys::Date::new_0().get_time();

        info!(format!("Fetched {} sessions in {:?} ms", sessions.len(), after - before));
        
        callback.emit(sessions);
    });
}

fn fetch_session(callback: Callback<TrackSession>, session_id: i64) {
    spawn_local(async move {
        let before = js_sys::Date::new_0().get_time();

        let Ok(response) = Request::get(&format!("/session/{session_id}")).send().await else {
            error!("Failed to fetch tracks");
            return;
        };

        let Ok(binary) = response.binary().await else {
            error!("Response was not binary");
            return;
        };

        let Ok(session) = bincode::deserialize::<TrackSession>(&binary) else {
            error!("Response was not binary");
            return;
        };

        let after = js_sys::Date::new_0().get_time();

        info!(format!("Fetched {} sessions in {:?} ms", session.track_points.len(), after - before));
        
        callback.emit(session);
    });
}

fn main() {
    yew::Renderer::<Model>::new().render();
}