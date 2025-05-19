use yew::prelude::*;
use yew_router::hooks::use_navigator;

use crate::{trip_data::TripData, Route};

#[derive(PartialEq, Properties, Clone)]
pub struct PanelProps {
    pub trip: Option<TripData>,
}

#[function_component]
pub fn PanelComponent(props: &PanelProps) -> Html {
    let navigator = use_navigator().unwrap();

    let on_click = Callback::from(move |_| {
        navigator.push(&Route::More);
    });

    html! {
        if let Some(trip_data) = &props.trip {
            <div class="panel component-container">
                <h1>{format!("{}", trip_data.trip.title)}</h1>
                <label>
                    {format!("{}", trip_data.trip.description)}
                </label>
                <h2>{format!("{} countries:", trip_data.trip.country_list.len())}</h2>
                <label>{
                    format!("{}", trip_data.trip.country_list.iter().map(|iso_a2| celes::Country::from_alpha2(iso_a2).unwrap().long_name).collect::<Vec<&str>>().join(", "))
                }</label>
                <label>{
                    format!("Currently in {}", trip_data.trip.country_list.last().map(|iso_a2| celes::Country::from_alpha2(iso_a2).unwrap().long_name).unwrap_or(&"???".to_owned()))
                }</label>
                <label>{
                    format!("Total distance: {} km", (trip_data.sessions.iter().map(|session| session.distance).sum::<f64>()) as u64)
                }</label>

                <div class="bottom-panel">
                    <label>{"Instagram: @silas_kavi @joachim_nebel"}</label>
                    <button onclick={on_click}>{"More"}</button>
                </div>
            </div>
        } else {
            <div class="panel component-container">
                <h1>{"No trip selected"}</h1>
            </div>
        }
    }
}