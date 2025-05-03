use gloo_console::info;
use trip_tracker_lib::trip::Trip;
use yew::prelude::*;

use crate::api::TripUpdate;

pub enum Msg {
    TripUpdated(TripUpdate),
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
                    <h2>{
                        format!("{} countries:", trip.country_list.len())
                    }</h2>
                    <label>{
                        format!("{}", trip.country_list.iter().map(|iso_a2| celes::Country::from_alpha2(iso_a2).unwrap().long_name).collect::<Vec<&str>>().join(", "))
                    }</label>
                    <h3>{
                        format!("Currently in {}", trip.country_list.last().map(|iso_a2| celes::Country::from_alpha2(iso_a2).unwrap().long_name).unwrap_or(&"???".to_owned()))
                    }</h3>
                </div>
            } else {
                <div class="panel component-container">
                    <h1>{"No trip selected"}</h1>
                </div>
            }
        }
    }
}