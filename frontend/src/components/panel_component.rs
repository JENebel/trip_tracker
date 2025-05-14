use yew::prelude::*;

use crate::trip_data::TripData;

#[derive(PartialEq, Properties, Clone)]
pub struct PanelProps {
    pub trip: Option<TripData>,
}

#[function_component]
pub fn PanelComponent(props: &PanelProps) -> Html {
    html! {
        if let Some(trip_data) = &props.trip {
            <div class="panel component-container">
                <h1>{format!("{}", trip_data.trip.title)}</h1>
                /*<label>
                    {format!("Trip id: {}", trip.trip_id)}
                </label>*/
                <label>
                    {format!("{}", trip_data.trip.description)}
                </label>
                <h2>{
                    format!("{} countries:", trip_data.trip.country_list.len())
                }</h2>
                <label>{
                    format!("{}", trip_data.trip.country_list.iter().map(|iso_a2| celes::Country::from_alpha2(iso_a2).unwrap().long_name).collect::<Vec<&str>>().join(", "))
                }</label>
                <h3>{
                    format!("Currently in {}", trip_data.trip.country_list.last().map(|iso_a2| celes::Country::from_alpha2(iso_a2).unwrap().long_name).unwrap_or(&"???".to_owned()))
                }</h3>
            </div>
        } else {
            <div class="panel component-container">
                <h1>{"No trip selected"}</h1>
            </div>
        }
    }
}