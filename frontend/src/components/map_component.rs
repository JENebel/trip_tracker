use gloo_console::{error, info};
use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, Polyline, PolylineOptions, TileLayer, TileLayerOptions};
use reqwasm::http::Request;
use trip_tracker_lib::track_session::TrackSession;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{js_sys::Array, Element, HtmlElement, Node};
use yew::prelude::*;

pub enum Msg {
    LoadTracks(Vec<TrackSession>),
}

pub struct MapComponent {
    map: Map,
    lat: Point,
    container: HtmlElement,
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
        Self::fetch_tracks(link.callback(Msg::LoadTracks));

        Self {
            map: leaflet_map,
            container,
            lat: props.pos,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.map.set_view(&LatLng::new(self.lat.0, self.lat.1), 7.0);
            add_tile_layer(&self.map);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::LoadTracks(tracks) => {
                let mut last_point = LatLng::new(0., 0.);
                for track in tracks {
                    info!("Adding track with {} points", track.track_points.len());
                    let opts = PolylineOptions::new();
                    opts.set_color("red".into());
                    opts.set_smooth_factor(1.5);
                    let points = track.track_points.iter().map(|tp| LatLng::new(tp.position.y(), tp.position.x()));

                    last_point = track.track_points.last().map(|tp| LatLng::new(tp.position.y(), tp.position.x())).unwrap();

                    Polyline::new_with_options(&Array::from_iter(points), &opts).add_to(&self.map);
                }

                self.map.pan_to(&last_point);
                
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
            </div>
        }
    }
}

fn add_tile_layer(map: &Map) {
    let url = "https://api.maptiler.com/maps/openstreetmap/256/{z}/{x}/{y}.jpg?key=";
    let key = include_str!("../../maptiler_key.txt").trim();
    let opts = TileLayerOptions::new();
    opts.set_update_when_idle(true);
    //opts.set_tile_size(512.);
    //opts.set_zoom_offset(-1.);
    TileLayer::new_options(&format!("{url}{key}"), &opts).add_to(map);
}
