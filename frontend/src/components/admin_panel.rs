use yew::{function_component, html, Html, Properties, use_state, Callback};
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct AdminProps;

#[function_component]
pub fn AdminPanel(_props: &AdminProps) -> Html {
    let token = use_state(|| "".to_string());
    let is_logged_in = use_state(|| false);
    let active_tab = use_state(|| "traffic".to_string());
    let traffic_data = use_state(|| vec![100, 200, 150, 300]); // mock data

    let on_token_input = {
        let token = token.clone();
        Callback::from(move |e: InputEvent| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            token.set(input.value());
        })
    };

    let on_login = {
        let token = token.clone();
        let is_logged_in = is_logged_in.clone();
        Callback::from(move |_| {
            if token.is_empty() {
                is_logged_in.set(true);
            }
        })
    };

    let set_tab = {
        let active_tab = active_tab.clone();
        move |tab: &'static str| {
            let active_tab = active_tab.clone();
            Callback::from(move |_| active_tab.set(tab.to_string()))
        }
    };

    html! {
        <div class="admin-panel">
            <h1>{ "Admin Panel" }</h1>
            <div class="login">
                <input 
                    class="token-input"
                    type="text"
                    placeholder="Admin token..."
                    value={(*token).clone()}
                    oninput={on_token_input}
                />
                <button onclick={on_login}>{ "Pull" }</button>
            </div>
            if *is_logged_in {
                <div class="dashboard">
                    <div class="tabs">
                        <button onclick={set_tab("traffic")} class={if *active_tab == "traffic" { "active" } else { "" }}>{ "Traffic" }</button>
                        <button onclick={set_tab("map")} class={if *active_tab == "map" { "active" } else { "" }}>{ "IP Map" }</button>
                    </div>
                    <div class="tab-content">
                        if *active_tab == "traffic" {
                            <TrafficGraph data={(*traffic_data).clone()} />
                        } else if *active_tab == "map" {
                            <IpMap />
                        }
                    </div>
                </div>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct GraphProps {
    pub data: Vec<u32>,
}

#[function_component]
fn TrafficGraph(props: &GraphProps) -> Html {
    html! {
        <div class="traffic-graph">
            { format!("Graph data: {:?}", props.data) }
        </div>
    }
}

#[function_component]
fn IpMap() -> Html {
    // Placeholder for a map — you'll integrate a JS-based map later
    html! {
        <div class="ip-map">
            { "Map showing IP locations (Coming soon)" }
        </div>
    }
}
