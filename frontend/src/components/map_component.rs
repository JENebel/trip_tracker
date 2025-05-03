use chrono::{FixedOffset, TimeZone, Utc};
use gloo_console::{error, info};
use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, Marker, Polyline, PolylineOptions, Popup, PopupOptions, TileLayer, TileLayerOptions, Tooltip, TooltipOptions};
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession, trip::Trip};
use wasm_bindgen::{prelude::wasm_bindgen, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{js_sys::Array, Element, HtmlElement, Node};
use yew::prelude::*;
use futures::future::join_all;

use crate::api;

pub enum Msg {
    LoadSession(TrackSession),
}

pub struct MapComponent {
    map: Map,
    map_center: Point,
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
    pub collapsed: bool,
    pub trip: Option<Trip>,
}

impl MapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
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

        Self {
            map: leaflet_map,
            container,
            map_center: props.pos,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.map.set_view(&LatLng::new(self.map_center.0, self.map_center.1), 8.0);
            add_tile_layer(&self.map);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadSession(session) => {
                if session.track_points.is_empty() {
                    return true;
                }

                let first_point = session.track_points.first().unwrap();
                let last_point = session.track_points.last().unwrap();

                info!(format!("Adding session {}({}) with {} points", &session.title, &session.session_id, session.track_points.len()));
                let opts = PolylineOptions::new();

                if session.active {
                    opts.set_color("rgb(41, 138, 67)".into());
                } else if session.session_id % 2 == 0 {
                    opts.set_color("rgb(0, 96, 255)".into());
                } else {
                    opts.set_color("rgb(0, 160, 255)".into());
                }
                
                opts.set_smooth_factor(1.5);
                opts.set_renderer(TOLERANT_RENDERER.with(JsValue::clone));

                /*let filtered_points = filter_anomalies(&session.track_points);
                let points = filtered_points.iter()*/
                let points = session.track_points.iter()
                    .map(|tp| LatLng::new(tp.latitude, tp.longitude));

                let last_lat_lng = LatLng::new(last_point.latitude, last_point.longitude);

                let zoom = self.map.get_zoom();
                self.map.set_view(&LatLng::new(last_lat_lng.lat(), last_lat_lng.lng()), zoom);

                /*if !session.active {
                    let popup_opts = PopupOptions::default();
                    let popup = Popup::new(&popup_opts, None);
                    popup.set_content(&format!("<b>Test marker</b>").into());
                    Marker::new(&last_lat_lng)
                        .bind_popup(&popup)
                        .add_to(&self.map);
                }*/

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
                let duration = (last_point.timestamp - first_point.timestamp).to_std().unwrap_or(Default::default());
                let hrs = duration.as_secs() / 3600;
                let mins = (duration.as_secs() % 3600) / 60;
                let time = format!("{:02}h {:02}m{}", hrs, mins, if session.active { " - Live" } else { "" });
                popup.set_content(&format!("<b>{}</b><br>{}<br>{}<br>{}<br>{}<br>Time zone: Copenhagen (+1)",
                    &session.title,
                    &session.session_id,
                    &FixedOffset::east_opt(1 * 3600).unwrap().from_utc_datetime(&first_point.timestamp.naive_utc()).format("%d/%m/%Y %H:%M").to_string(),
                    time,
                    session.description
                ).into());

                Polyline::new_with_options(&Array::from_iter(points), &opts)
                    .bind_tooltip(&tooltip)
                    .bind_popup(&popup)
                    .add_to(&self.map);

                true
            }
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        self.map.invalidate_size(false);
        let props = ctx.props();

        // Fetch track sessions on creation and send them via a message
        if let Some(trip) = &props.trip {
            get_trip_sessions(trip.trip_id, ctx.link().callback(Msg::LoadSession));
        }

        if self.map_center == props.pos {
            false
        } else {
            self.map_center = props.pos;
            self.map.set_view(&LatLng::new(self.map_center.0, self.map_center.1), 10.0);
            true
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <div class="map">
                {self.render_map()}
                // Button in top left
                //<button class="leaflet-top leaflet-right">
                    
                //</button>
            </div>
        }
    }
}

fn filter_anomalies(points: &Vec<TrackPoint>) -> Vec<TrackPoint> {
    let mut filtered_points = Vec::new();
    // Filter out points that are very far from its neighbors
    for i in 1..points.len() - 1 {
        let prev_point = &points[i - 1];
        let next_point = &points[i + 1];
        let curr_point = &points[i];

        // Calculate the distance between the two points
        let dist_to_prev = haversine_distance(prev_point.latitude, prev_point.longitude, curr_point.latitude, curr_point.longitude);
        let dist_to_next = haversine_distance(curr_point.latitude, curr_point.longitude, next_point.latitude, next_point.longitude);
        let distance = dist_to_prev + dist_to_next;

        // If the distance is too large, skip this point
        if distance * 5. > haversine_distance(prev_point.latitude, prev_point.longitude, next_point.latitude, next_point.longitude) {
            continue;
        }

        filtered_points.push(curr_point.clone());
    }
    filtered_points
}

fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // Radius of the Earth in kilometers
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c // Distance in kilometers
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

fn get_trip_sessions(trip_id: i64, callback: Callback<TrackSession>) {
    spawn_local(async move {
        let Ok(sessions) = api::get_trip_session_ids(trip_id).await else {
            error!("Failed to get trip sessions");
            return;
        };

        let futures = sessions.iter().map(|&id| api::get_session(id));
        let results = join_all(futures).await;

        for res in results {
            if let Ok(session) = res {
                callback.emit(session);
            }
        }
    });
}