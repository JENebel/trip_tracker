use std::collections::HashMap;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use gloo_console::info;
use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, Polyline, PolylineOptions, Popup, PopupOptions, TileLayer, TileLayerOptions, Tooltip, TooltipOptions};
use trip_tracker_lib::track_session::TrackSession;
use wasm_bindgen::{prelude::wasm_bindgen, JsCast, JsValue};
use web_sys::{js_sys::Array, Element, HtmlElement, Node};
use yew::prelude::*;

use crate::trip_data::TripData;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(thread_local_v2)]
    static TOLERANT_RENDERER: JsValue;
}

pub struct MapComponent {
    map: Map,
    container: HtmlElement,
    polylines: HashMap<i64, Polyline>,
    most_recent_time: DateTime<Utc>,
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub trip_data: Option<TripData>,
    pub collapsed: bool,
}

impl MapComponent {
    fn render_map(&self) -> Html {
        let node: &Node = &self.container.clone().into();
        Html::VRef(node.clone())
    }
}

impl Component for MapComponent {
    type Message = ();
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        let container: Element = document().create_element("div").unwrap();
        let container: HtmlElement = container.dyn_into().unwrap();
        container.set_class_name("map");

        let leaflet_map = Map::new_with_element(&container, &MapOptions::default());

        Self {
            map: leaflet_map,
            container,
            polylines: HashMap::new(),
            most_recent_time: DateTime::from_timestamp_nanos(0)
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.map.set_max_zoom(25.);
            self.map.set_view(&LatLng::new(56.175188, 10.196123), 8.0);
            add_tile_layer(&self.map);
        }
    }

    fn changed(&mut self, ctx: &Context<Self>, old_props: &Self::Properties) -> bool {
        self.map.invalidate_size(false);
        let props = ctx.props();

        let old_trip_data = &old_props.trip_data;

        // Find out what sessions changed, if any, and update them on the map
        if let Some(trip_data) = &props.trip_data {
            for session in &trip_data.sessions {
                if let Some(Some(old_session)) = old_trip_data.as_ref().map(|td| td.sessions.iter().find(|s| s.session.session_id == session.session.session_id)) {
                    // Replace session
                    if old_session != session {
                        if let Some(existing) = self.polylines.get(&session.session.session_id) {
                            let mut n = 0;
                            for i in old_session.session.track_points.len()..session.session.track_points.len() {
                                let pt = &session.session.track_points[i];
                                existing.add_lat_lng(&LatLng::new(pt.latitude, pt.longitude));
                                n += 1;
                            }

                            if n > 0 {
                                info!("Added {} points", n);
                            }
                        }
                    }

                    if !session.session.active {
                        if let Some(existing) = self.polylines.get(&session.session.session_id) {
                            let opts = PolylineOptions::new();

                            // Ugly code duplication!! ):
                            if session.session.session_id % 2 == 0 {
                                opts.set_color("rgb(0, 96, 255)".into());
                            } else {
                                opts.set_color("rgb(0, 160, 255)".into());
                            }
                            
                            opts.set_smooth_factor(1.5);
                            opts.set_renderer(TOLERANT_RENDERER.with(JsValue::clone));

                            existing.set_style(&opts);
                        }
                    }
                } else {
                    // Add session
                    let polyline = make_polyline(&session.session);
                    update_metadata(&polyline, &session.session, session.distance);
                    polyline.add_to(&self.map);
                    self.polylines.insert(session.session.session_id, polyline);
                    if let Some(last_point) = session.session.track_points.last() {
                        if last_point.timestamp > self.most_recent_time {
                            self.most_recent_time = last_point.timestamp;
                            let zoom = self.map.get_zoom();
                            self.map.set_view(&LatLng::new(last_point.latitude, last_point.longitude), zoom);
                        }
                    }
                    info!("Added new session")
                }
            }
        } else {
            for feature in self.polylines.values() {
                feature.remove();
            }
        }

        true
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

fn update_metadata(polyline: &Polyline, track_session: &TrackSession, distance :f64) {
    if track_session.track_points.len() < 1 {
        return;
    }

    let first_point = track_session.track_points.first().unwrap();
    let last_point = track_session.track_points.last().unwrap();

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
    let duration = last_point.timestamp.signed_duration_since(first_point.timestamp).to_std().unwrap_or(Default::default());
    let hrs = duration.as_secs() / 3600;
    let mins = (duration.as_secs() % 3600) / 60;
    let time = format!("{:02}h {:02}m{}", hrs, mins, if track_session.active { " - Live" } else { "" });

    let distance = format!("{:.1}{}", if distance > 1. {distance} else {distance * 1000.}, if distance > 1. { " km" } else { " m" });
    popup.set_content(&format!("<b>{}</b><br>{}<br>{}<br>{}<br>{}<br>Time zone: Copenhagen (+1)",
        &track_session.title,
        &FixedOffset::east_opt(1 * 3600).unwrap().from_utc_datetime(&first_point.timestamp.naive_utc()).format("%d/%m/%Y %H:%M").to_string(),
        time,
        distance,
        track_session.description
    ).into());

    polyline.bind_tooltip(&tooltip)
    .bind_popup(&popup);
}

fn make_polyline(track_session: &TrackSession) -> Polyline {
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

    let points = track_session.track_points.iter()
        .map(|tp| LatLng::new(tp.latitude, tp.longitude));

    Polyline::new_with_options(&Array::from_iter(points), &opts)
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