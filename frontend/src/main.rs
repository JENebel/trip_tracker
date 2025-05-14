use crate::components::{
    map_component::MapComponent,
};
use components::panel_component::PanelComponent;
use futures::future::join_all;
use gloo_console::{error, info};
use gloo_timers::callback::Interval;
use trip_data::TripData;
use util::filter_anomalies;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::{
    history::{BrowserHistory, History}, BrowserRouter, Routable, Switch}
;

mod api;
mod components;
mod trip_data;
mod util;

#[derive(Clone, Debug, PartialEq, Routable)]
enum Route {
    #[at("/:id")]
    Trip { id: i64 },
    #[at("/")]
    Default,
    #[at("/admin")]
    Admin,
    #[at("/admin/:id")]
    TripAdmin { id: i64 },
    #[not_found]
    #[at("/404")]
    Invalid,
}

impl Route {
    fn parse(path: &str) -> Self {
        info!("Path", path);
        if path == "/" {
            Self::Default
        } else {
            match path.trim_start_matches('/').parse::<i64>() {
                Ok(id) => Self::Trip { id },
                Err(_) => Self::Invalid
            }
        }
    }
}

fn load_default_trip(trip_cb: Callback<Option<TripData>>) {
    spawn_local(async move {
        if let Ok(trip_id) = api::get_default_trip_id().await {
            load_trip_data(trip_id, trip_cb);
        }
    });
}

fn load_trip_data(trip_id: i64, trip_cb: Callback<Option<TripData>>) {
    spawn_local(async move {
        let Ok(trip) = api::get_trip(trip_id).await else {
            return 
        };

        let Ok(sessions) = api::get_trip_session_ids(trip_id).await else {
            error!("Failed to get trip sessions");
            return;
        };

        let futures = sessions.iter().map(|&id| api::get_session(id));
        let results = join_all(futures).await;

        let mut sessions = Vec::new();

        results.into_iter().filter_map(|r| r.ok()).for_each(|session| {
            sessions.push(filter_anomalies(session));
        });

        let td = TripData {
            trip,
            sessions,
        };

        trip_cb.emit(Some(td));
    });
}

fn poll_for_updates(trip_data: UseStateHandle<Option<TripData>>) {
    let interval = Interval::new(5000, move || {
        let trip_data_handle = trip_data.clone();

        spawn_local(async move {
            info!("Fetch update");

            let Some(mut trip_data) = (*trip_data_handle).clone() else {
                info!(format!("No trip selected {:?}", trip_data_handle));
                return
            };

            let Ok(trip) = api::get_trip(trip_data.trip.trip_id).await else {
                return
            };

            // Update trip metadata
            if trip != trip_data.trip {
                trip_data.trip = trip;
            }

            // Update sessions
            let Ok(session_ids) = api::get_trip_session_ids(trip_data.trip.trip_id).await else {
                error!("Failed to get trip sessions");
                return;
            };

            for id in session_ids {
                match trip_data.sessions.iter_mut().find(|s| s.session_id == id) {
                    Some(existing) => {
                        if existing.active{
                            let current_points = existing.track_points.len();
                            if let Ok(update) = api::get_session_update(existing.session_id, current_points).await {
                                existing.track_points.extend(update.new_track_points);
                                existing.description = update.description;
                                existing.title = update.title;
                                existing.active = update.still_active;
                            }
                        }
                    },
                    None => {
                        if let Ok(session) = api::get_session(id).await {
                            trip_data.sessions.push(session);
                        }
                    },
                }
            }

            trip_data_handle.set(Some(trip_data));
        });
    });
    std::mem::forget(interval);
}

#[function_component]
fn App() -> Html {
    let trip_data: UseStateHandle<Option<TripData>> = use_state(|| None);

    let trip_clone = trip_data.clone();
    use_effect_with((), move |_| {
        poll_for_updates(trip_clone);
    });

    let is_first_render = use_state(|| true);
    if *is_first_render {
        let history = BrowserHistory::new();
        let location = history.location();
        let route = Route::parse(location.path());
        match &route {
            Route::Default => {
                info!("Loading default trip");
                let trip_data = trip_data.clone();
                load_default_trip(Callback::from(move |new_trip| trip_data.set(new_trip)));
            }
            Route::Trip { id } => {
                info!(format!("Loading trip with ID: {}", id));
                let trip_data = trip_data.clone();
                load_trip_data(*id, Callback::from(move |new_trip| trip_data.set(new_trip)));
            }
            _ => {}
        }

        is_first_render.set(false);
    }

    let collapsed = use_state(|| false);
    let toggle_collapsed = {
        let collapsed = collapsed.clone();
        Callback::from(move |_| collapsed.set(!*collapsed))
    };

    html! {
        <BrowserRouter>
            <Switch<Route> render={move |r| {
                match r {
                Route::Trip { id: _ } | Route::Default => html!{<>
                    if !*collapsed {
                        <PanelComponent trip={(*trip_data).clone()} />
                    }
                    if *collapsed {
                        <button onclick={toggle_collapsed.clone()} class="collapse-btn-vert collapse-btn">
                            {"▶"}
                        </button>
                        <button onclick={toggle_collapsed.clone()} class="collapse-btn-horiz collapse-btn">
                            {"▼"}
                        </button>
                    } else {
                        <button onclick={toggle_collapsed.clone()} class="collapse-btn-vert collapse-btn">
                            {"◀"}
                        </button>
                        <button onclick={toggle_collapsed.clone()} class="collapse-btn-horiz collapse-btn">
                            {"▲"}
                        </button>
                    }
                    <MapComponent trip_data={(*trip_data).clone()} />
                    </>},
                Route::Admin => /* html! { <AdminPanel /> }*/ html!("Admin"),
                Route::TripAdmin { id: _ } => html!("Trip admin"),
                Route::Invalid => html!("Invalid"),
            }}} />
        </BrowserRouter>
    }
}

fn main() {
    let handle = yew::Renderer::<App>::new().render();
    
}