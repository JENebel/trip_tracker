use gloo_console::{error, info};
use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, Marker, Polyline, PolylineOptions, Popup, PopupOptions, TileLayer, TileLayerOptions, Tooltip, TooltipOptions};
use reqwasm::http::Request;
use trip_tracker_lib::track_session::TrackSession;
use wasm_bindgen::{prelude::wasm_bindgen, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{js_sys::{self, Array}, Element, HtmlElement, Node};
use yew::prelude::*;

pub enum Msg {
    LoadSessions(Vec<TrackSession>),
}

pub struct MapComponent {
    map: Map,
    lat: Point,
    container: HtmlElement,
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(thread_local_v2)]
    static TOLERANT_RENDERER: JsValue;
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Point(pub f64, pub f64);

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub pos: Point,
}

impl MapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }

    fn fetch_tracks(callback: Callback<Vec<TrackSession>>) {
        spawn_local(async move {
            let before = js_sys::Date::new_0().get_time();

            let Ok(response) = Request::get("/tracks").send().await else {
                error!("Failed to fetch tracks");
                return;
            };

            let Ok(binary) = response.binary().await else {
                error!("Response was not binary");
                return;
            };

            let Ok(sessions) = bincode::deserialize::<Vec<TrackSession>>(&binary) else {
                error!("Response was not binary");
                return;
            };

            let after = js_sys::Date::new_0().get_time();

            info!(format!("Fetched {} tracks in {:?} ms", sessions.len(), after - before));
            
            callback.emit(sessions);
        });
    }
}

impl Component for MapComponent {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        let link = ctx.link().clone();

        let container: Element = document().create_element("div").unwrap();
        let container: HtmlElement = container.dyn_into().unwrap();
        container.set_class_name("map");

        let leaflet_map = Map::new_with_element(&container, &MapOptions::default());

        // Fetch track sessions on creation and send them via a message
        Self::fetch_tracks(link.callback(Msg::LoadSessions));

        Self {
            map: leaflet_map,
            container,
            lat: props.pos,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.map.set_view(&LatLng::new(self.lat.0, self.lat.1), 8.0);
            add_tile_layer(&self.map);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadSessions(sessions) => {
                let mut total_points = 0;
                for (i, session) in sessions.iter().enumerate() {
                    if session.track_points.is_empty() {
                        continue;
                    }

                    let first_point = session.track_points.first().unwrap();
                    let last_point = session.track_points.last().unwrap();

                    let is_last_session = i == sessions.len() - 1;
                    info!(format!("Adding track {} with {} points", &session.title, session.track_points.len()));
                    let opts = PolylineOptions::new();

                    if session.active {
                        opts.set_color("rgb(41, 138, 67)".into());
                    } else if i % 2 == 0 {
                        opts.set_color("rgb(245, 76, 76)".into());
                    } else {
                        opts.set_color("rgb(76, 141, 245)".into());
                    }
                    
                    opts.set_smooth_factor(1.5);
                    opts.set_renderer(TOLERANT_RENDERER.with(JsValue::clone));
                    let points = session.track_points.iter().map(|tp| LatLng::new(tp.position.y(), tp.position.x()));

                    let last_lat_lng = LatLng::new(last_point.position.y(), last_point.position.x());

                    if is_last_session {
                        self.map.pan_to(&last_lat_lng);
                    }

                    if session.active {
                        
                    } else {
                        let popup_opts = PopupOptions::default();
                        let popup = Popup::new(&popup_opts, None);
                        popup.set_content(&format!("<b>Test marker</b>").into());
                        Marker::new(&last_lat_lng)
                            .bind_popup(&popup)
                            .add_to(&self.map);
                    }

                    let tooltip_opts = TooltipOptions::default();
                    tooltip_opts.set_sticky(true);
                    tooltip_opts.set_direction("bottom".into());
                    let tooltip = Tooltip::new(&tooltip_opts, None);
                    tooltip.set_content(&format!("{}<br>{}",
                        &session.title,
                        &if session.active {"On the move".into()} else {first_point.timestamp.format("%d/%m/%Y").to_string()}
                    ).into());

                    let popup_opts = PopupOptions::default();
                    let popup = Popup::new(&popup_opts, None);
                    let duration = (last_point.timestamp - first_point.timestamp).to_std().unwrap();
                    let hrs = duration.as_secs() / 3600;
                    let mins = (duration.as_secs() % 3600) / 60;
                    let time = format!("{:02}h {:02}m{}", hrs, mins, if session.active { " - Live" } else { "" });
                    popup.set_content(&format!("<b>{}</b><br>{}<br>{}<br>{}",
                        &session.title,
                        &first_point.timestamp.format("%d/%m/%Y %H:%M").to_string(),
                        time,
                        session.description
                    ).into());

                    Polyline::new_with_options(&Array::from_iter(points), &opts)
                        .bind_tooltip(&tooltip)
                        .bind_popup(&popup)
                        .add_to(&self.map);

                    total_points += session.track_points.len();
                }

                info!(format!("Added {} total points", total_points));
                
                true
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if self.lat == props.pos {
            false
        } else {
            self.lat = props.pos;
            self.map
                .set_view(&LatLng::new(self.lat.0, self.lat.1), 10.0);
            true
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <div class="map">
                {self.render_map()}
                // Button in top left
                <button class="leaflet-top leaflet-right">
                    
                </button>
            </div>
        }
    }
}

fn add_tile_layer(map: &Map) {
    let key = include_str!("../../maptiler_key.txt").trim();
    //let url = format!("https://api.maptiler.com/maps/openstreetmap/256/{{z}}/{{x}}/{{y}}.jpg?key={}", key);
    let url = format!("https://api.maptiler.com/maps/basic-v2/256/{{z}}/{{x}}/{{y}}.png?key={}", key);
    //let url = "	https://tile.openstreetmap.org/{z}/{x}/{y}.png";
    let opts = TileLayerOptions::new();
    opts.set_update_when_idle(true);
    TileLayer::new_options(&url, &opts).add_to(map);
}
