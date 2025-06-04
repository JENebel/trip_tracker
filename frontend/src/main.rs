use std::time::Duration;

use crate::components::map_component::MapComponent;
use components::panel_component::PanelComponent;
use futures::future::join_all;
use gloo_console::{error, info};
use gloo_timers::future::sleep;
use trip_data::{SessionData, TripData};
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
    #[at("/more")]
    More,
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

fn load_default_trip(trip_cb: Callback<TripData>) {
    spawn_local(async move {
        if let Ok(trip_id) = api::get_default_trip_id().await {
            load_trip_data(trip_id, trip_cb);
        }
    });
}

fn load_trip_data(trip_id: i64, trip_cb: Callback<TripData>) {
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

        let mut sessions: Vec<SessionData> = Vec::new();

        results.into_iter().filter_map(|r| r.ok()).for_each(|session| {
            sessions.push(SessionData::from_session(filter_anomalies(session)));
        });

        sessions.sort_by_key(|s| s.session.track_points.first().map(|p| p.timestamp.timestamp()).unwrap_or(0));

        let mut trip_data = TripData {
            trip,
            sessions,
        };

        trip_cb.emit(trip_data.clone());

        loop {
            sleep(Duration::from_secs(5)).await;
            
            info!("Fetch update");

            let Ok(trip) = api::get_trip(trip_data.trip.trip_id).await else {
                error!("Failed to get trip");
                continue
            };

            // Update trip metadata
            if trip != trip_data.trip {
                trip_data.trip = trip;
            }

            // Update sessions
            let Ok(session_ids) = api::get_trip_session_ids(trip_data.trip.trip_id).await else {
                error!("Failed to get trip sessions");
                continue;
            };

            for id in session_ids {
                match trip_data.sessions.iter_mut().find(|s| s.session.session_id == id) {
                    Some(existing) => {
                        if existing.session.active{
                            let current_points = existing.session.track_points.len();
                            if let Ok(update) = api::get_session_update(existing.session.session_id, current_points).await {
                                existing.session.track_points.extend(update.new_track_points);
                                existing.session.description = update.description;
                                existing.session.title = update.title;
                                existing.session.active = update.still_active;
                            }
                            existing.distance = existing.session.distance()
                        }
                    },
                    None => {
                        if let Ok(session) = api::get_session(id).await {
                            trip_data.sessions.push(SessionData::from_session(session));
                        }
                    },
                }
            }


            trip_cb.emit(trip_data.clone());
        };
    });
}

#[function_component]
fn App() -> Html {
    let trip_data: UseStateHandle<Option<TripData>> = use_state(|| None);

    let is_first_render = use_state(|| true);
    if *is_first_render {
        let history = BrowserHistory::new();
        let location = history.location();
        let route = Route::parse(location.path());
        match &route {
            Route::Default => {
                info!("Loading default trip");
                let trip_data = trip_data.clone();
                load_default_trip(Callback::from(move |new_trip| trip_data.set(Some(new_trip))));
            }
            Route::Trip { id } => {
                info!(format!("Loading trip with ID: {}", id));
                let trip_data = trip_data.clone();
                load_trip_data(*id, Callback::from(move |new_trip| trip_data.set(Some(new_trip))));
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
                    <MapComponent trip_data={(*trip_data).clone()} collapsed={*collapsed} />
                    </>},
                Route::Admin => /* html! { <AdminPanel /> }*/ html!("Admin"),
                Route::TripAdmin { id: _ } => html!("Trip admin"),
                Route::Invalid => html!("Invalid"),
                Route::More => html! {
                    <div style="padding: 20px; font-family: Arial, sans-serif;">
                        <h3 style="margin-bottom: 20px;">
                            {"Special thanks"}
                        </h3>
                        <div style="margin-bottom: 20px;">
                            {"- Highland Hostel Yerevan. Tigran, the owner, for facilitating the purchase of the Lada. And Sean and Mehrzatt, for their help and hospitality."}
                        </div>
                        <div style="margin-bottom: 20px;">
                            {"- Jens "}
                            <a href="https://instagram.com/overlandtour" target="_blank">{"@overlandtour"}</a>
                            {", for sharing his experiences and tips from a similar project."}
                        </div>

                        <h3 style="margin-bottom: 20px;">
                            {"More"}
                        </h3>
                        <div style="line-height: 1.6;">
                            <p>{"Yes, \"Tour de Lada\" is grammatically correct and has a fun, stylish flair—especially if you're aiming for a playful or ironic tone, similar to phrases like Tour de France. It's borrowing French structure (\"Tour de X\"), which is often used in English for effect, even if the rest of the sentence isn't in French.
                                So if you're embarking on a trip involving a Lada (the car), calling it a \"Tour de Lada\" works well, especially for social media, blogs, or if you're naming the trip as an event."}</p>
                        </div>
                        <div style="margin-bottom: 20px;">
                            <a href="https://github.com/JENebel/trip_tracker" target="_blank">{"GitHub"}</a>
                        </div>
                    </div>
                }
            }}} />
        </BrowserRouter>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}