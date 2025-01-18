use gloo_utils::document;
use leaflet::{LatLng, Map, MapOptions, TileLayer};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, Node};
use yew::prelude::*;

pub enum Msg {}

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
}

impl Component for MapComponent {
    type Message = Msg;
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
            lat: props.pos,
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, first_render: bool) {
        if first_render {
            self.map
                .set_view(&LatLng::new(self.lat.0, self.lat.1), 11.0);
            add_tile_layer(&self.map);
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        false
    }

    fn changed(&mut self, ctx: &Context<Self>, _old_props: &Self::Properties) -> bool {
        let props = ctx.props();

        if self.lat == props.pos {
            false
        } else {
            self.lat = props.pos;
            self.map
                .set_view(&LatLng::new(self.lat.0, self.lat.1), 11.0);
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
    TileLayer::new("https://{s}.tile.osm.org/{z}/{x}/{y}.png").add_to(map);
}