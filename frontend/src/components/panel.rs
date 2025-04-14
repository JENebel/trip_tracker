use yew::prelude::*;

pub enum Msg {
    //TripChosen(i64),
}

pub struct Panel {
    
}

#[derive(PartialEq, Properties, Clone)]
pub struct Props {
    pub select_trip: Callback<Option<i64>>,
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

    fn view(&self, _ctx: &Context<Self>) -> Html {
        html! {
            <div class="control component-container">
                <h1>{"Demo"}</h1>
                <div>
                    </div>

            </div>
        }
    }
}