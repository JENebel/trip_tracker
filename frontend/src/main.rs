use crate::components::{
    control::Control,
    map_component::{MapComponent, Point},
};
use yew::prelude::*;
mod components;

enum Msg {
    //SetViewLocation(Point),
    SetTrip(i64),
}

struct Model {
    selected_trip: i64,
}

impl Component for Model {
    type Message = Msg;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        // Get the latest track session from the server
        Self { selected_trip: 0 }
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
                <Control select_trip={cb} />
                <MapComponent pos={point} />
            </>
        }
    }
}

fn main() {
    yew::Renderer::<Model>::new().render();
}