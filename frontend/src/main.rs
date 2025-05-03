use crate::components::{
    map_component::{MapComponent, Point},
    panel::Panel,
};
use components::admin_panel::AdminPanel;
use gloo_console::{error, info};
use trip_tracker_lib::trip::Trip;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::{
    history::{BrowserHistory, History}, BrowserRouter, Routable, Switch}
;

mod api;
mod components;

#[derive(Clone, Debug, PartialEq, Routable)]
enum Route {
    #[at("/{id}")]
    Trip { id: i64 },
    #[at("/")]
    Default,
    #[at("/admin")]
    Admin,
    #[at("/admin/{id}")]
    TripAdmin { id: i64 },
    #[not_found]
    #[at("/404")]
    Invalid,
}

impl Route {
    fn parse(path: &str) -> Self {
        if path == "/" {
            Self::Default
        } else {
            match path.trim_start_matches('/').parse::<i64>() {
                Ok(id) => Self::Trip { id },
                Err(_) => Self::Invalid,
            }
        }
    }
}

enum MainMsg {
    SelectTrip(Option<Trip>),
    ToggleCollapsed,
}

struct Model {
    selected_trip: Option<Trip>,
    collapsed: bool,
}

impl Component for Model {
    type Message = MainMsg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link().clone();

        let history = BrowserHistory::new();
        let location = history.location();
        let route = Route::parse(location.path());

        match route {
            Route::Default => {
                info!("Default route");
                let cb = link.callback(MainMsg::SelectTrip);
                spawn_local(async move {
                    if let Ok(trip) = api::get_default_trip().await {
                        cb.emit(Some(trip));
                    }
                });
            }
            Route::Trip { id } => {
                info!(format!("Trip route: {}", id));
                let cb = link.callback(MainMsg::SelectTrip);
                spawn_local(async move {
                    if let Ok(trip) = api::get_trip(id).await {
                        cb.emit(Some(trip));
                    }
                });
            }
            Route::Admin | Route::TripAdmin{id: _} => {
                // Do nothing here
            }
            Route::Invalid => {
                error!("Invalid route");
            },
        };

        Self {
            selected_trip: None,
            collapsed: false,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            MainMsg::SelectTrip(trip) => {
                info!(format!("Selected trip: {:?}", trip));
                self.selected_trip = trip;
            }
            MainMsg::ToggleCollapsed => {
                info!(format!("Toggle collapsed"));
                self.collapsed = !self.collapsed;
            }
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let collapsed = self.collapsed;
        let ctx = ctx.link().clone();

        let point = Point(56.175188, 10.196123);

        let select_cb = ctx.callback(move |trip_id: Option<Trip>| MainMsg::SelectTrip(trip_id));
        let on_click_cb = ctx.callback(move |()| MainMsg::ToggleCollapsed);

        let selected_trip = self.selected_trip.clone();

        html! {
            <BrowserRouter>
                <Switch<Route> render={move |r| match r {
                    Route::Trip { id: _ } | Route::Default => html!{<>
                        if !collapsed {
                            <Panel select_trip={select_cb.clone()} selected_trip={selected_trip.clone()} />
                        }
                        <CollapseBtn collapsed={collapsed} on_click={on_click_cb.clone()} />
                        <MapComponent pos={point} collapsed={collapsed} trip={selected_trip.clone()} />
                        </>},
                    Route::Admin => html! { <AdminPanel /> },
                    Route::TripAdmin { id } => todo!(),
                    Route::Invalid => todo!(),
                }} />
            </BrowserRouter>
        }
    }
}

#[derive(PartialEq, Properties, Clone)]
struct CollapseBtnProps {
    collapsed: bool,
    on_click: Callback<()>,
}

#[function_component]
fn CollapseBtn(props: &CollapseBtnProps) -> Html {
    let on_click_clone = props.on_click.clone();

    let onclick = Callback::from(move |_| {
        on_click_clone.emit(());
    });

    if props.collapsed {
        html! { <>
            <button onclick={onclick.clone()} class="collapse-btn-vert collapse-btn">
                {"▶"}
            </button>
            <button onclick={onclick.clone()} class="collapse-btn-horiz collapse-btn">
                {"▼"}
            </button>
        </> }
    } else {
        html! { <>
            <button onclick={onclick.clone()} class="collapse-btn-vert collapse-btn">
                {"◀"}
            </button>
            <button onclick={onclick.clone()} class="collapse-btn-horiz collapse-btn">
                {"▲"}
            </button>
        </> }
    }
}

fn main() {
    yew::Renderer::<Model>::new().render();
}
