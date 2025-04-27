use gloo_console::info;
use trip_tracker_lib::trip::Trip;
use yew::prelude::*;

pub enum Msg {
    TripChanged(Option<i64>),
}

pub struct Panel {
    
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub select_trip: Callback<Option<Trip>>,
    pub selected_trip: Option<Trip>,
}

impl Panel {
    /*fn button(&self, ctx: &Context<Self>, city: City) -> Html {
        let name = city.name.clone();
        let cb = ctx.link().callback(move |_| Msg::CityChosen(city.clone()));
        html! {
            <button onclick={cb}>{name}</button>
        }
    }*/
}

impl Component for Panel {
    type Message = Msg;
    type Properties = Props;

    fn create(_ctx: &Context<Self>) -> Self {
        Panel {
            //cities: ctx.props().cities.list.clone(),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, _msg: Self::Message) -> bool {
        /*match msg {
            Msg::TripChosen(trip) => {
                log!(format!("Update: {:?}", trip));
                ctx.props().select_trip.emit(trip);
            }
        }*/
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let props = ctx.props();
        if let Some(trip) = &props.selected_trip {
            info!(format!("Selected trip description: {:?}", trip.description));
        }
        html! {
            if let Some(trip) = &props.selected_trip {
                <div class="panel component-container">
                    <h1>{format!("{}", trip.title)}</h1>
                    <label>
                        {format!("Trip id: {}", trip.trip_id)}
                    </label>
                    <label>
                        {format!("{}", trip.description)}
                    </label>
                </div>
            } else {
                <div class="panel component-container">
                    <h1>{"No trip selected"}</h1>
                </div>
            }
        }
    }
}