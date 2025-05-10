use chrono::{FixedOffset, TimeZone};
use gloo_console::{error, info};
use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, Polyline, PolylineOptions, Popup, PopupOptions, TileLayer, TileLayerOptions, Tooltip, TooltipOptions};
use trip_tracker_lib::{track_point::TrackPoint, track_session::TrackSession, trip::Trip};
use wasm_bindgen::{prelude::wasm_bindgen, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::{js_sys::Array, Element, HtmlElement, Node};
use yew::prelude::*;
use futures::future::join_all;

use crate::{api, trip_data::{TripData, TripMessage}};

pub struct MapComponent {
    map: Map,
    map_center: Point,
    container: HtmlElement,
    selected_trip: Option<TripData>,
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
    pub trip: Option<TripData>,
}

impl MapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }
}

impl Component for MapComponent {
    type Message = TripMessage;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        let props = ctx.props();
        let container: Element = document().create_element("div").unwrap();
        let container: HtmlElement = container.dyn_into().unwrap();
        container.set_class_name("map");

        let leaflet_map = Map::new_with_element(&container, &MapOptions::default());

        Self {
            map: leaflet_map,
            container,
            map_center: props.pos,
            selected_trip: None,
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
            Msg::LoadSession(track_session) => {
                if track_session.track_points.is_empty() {
                    return true;
                }

                let first_point = track_session.track_points.first().unwrap();
                let last_point = track_session.track_points.last().unwrap();

                info!(format!("Adding session {}({}) with {} points", &track_session.title, &track_session.session_id, track_session.track_points.len()));
                let opts = PolylineOptions::new();

                if track_session.active {
                    opts.set_color("rgb(41, 138, 67)".into());
                } else if track_session.session_id % 2 == 0 {
                    opts.set_color("rgb(0, 96, 255)".into());
                } else {
                    opts.set_color("rgb(0, 160, 255)".into());
                }
                
                opts.set_smooth_factor(1.5);
                opts.set_renderer(TOLERANT_RENDERER.with(JsValue::clone));

                let filtered_points = filter_anomalies(&track_session.track_points);
                let points = filtered_points.iter()
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
                    &track_session.title,
                    &if track_session.active {"On the move".into()} else {first_point.timestamp.format("%d/%m/%Y").to_string()}
                ).into());

                let popup_opts = PopupOptions::default();
                let popup = Popup::new(&popup_opts, None);
                let duration = (last_point.timestamp - first_point.timestamp).to_std().unwrap_or(Default::default());
                let hrs = duration.as_secs() / 3600;
                let mins = (duration.as_secs() % 3600) / 60;
                let time = format!("{:02}h {:02}m{}", hrs, mins, if track_session.active { " - Live" } else { "" });
                popup.set_content(&format!("<b>{}</b><br>{}<br>{}<br>{}<br>{}<br>Time zone: Copenhagen (+1)",
                    &track_session.title,
                    &track_session.session_id,
                    &FixedOffset::east_opt(1 * 3600).unwrap().from_utc_datetime(&first_point.timestamp.naive_utc()).format("%d/%m/%Y %H:%M").to_string(),
                    time,
                    track_session.description
                ).into());

                let polyline = Polyline::new_with_options(&Array::from_iter(points), &opts)
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
        if self.selected_trip != props.trip {
            if let Some(trip) = &props.trip {
                self.sessions.clear();
                get_trip_sessions(trip.trip_id, ctx.link().callback(Msg::LoadSession));
            }
            self.selected_trip = props.trip.clone();
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
        let curr_point = &points[i];
        let next_point = &points[i + 1];

        // Calculate the distance between the two points
        let dist_to_prev = haversine_distance((prev_point.latitude, prev_point.longitude), (curr_point.latitude, curr_point.longitude));
        let dist_to_next = haversine_distance((curr_point.latitude, curr_point.longitude), (next_point.latitude, next_point.longitude));
        let min_dist = dist_to_prev.max(dist_to_next);
        let dist_between_neighbors = haversine_distance((prev_point.latitude, prev_point.longitude), (next_point.latitude, next_point.longitude));
        //info!(format!("Min dist: {}, distance between neighbors: {}", dist_to_prev, dist_between_neighbors));

        // If the distance is too large, skip this point
        if min_dist > dist_between_neighbors * 5.0 {
            continue;
        }

        filtered_points.push(curr_point.clone());
    }
    
    info!(format!("Filtered away {}", points.len() - filtered_points.len()));

    filtered_points
}

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