use axum::{
    body::Bytes, extract::{
        ws::{Message, WebSocket, WebSocketUpgrade}, State
    }, response::IntoResponse, routing::get, Router
};
use futures::{sink::SinkExt, stream::StreamExt};
use local_ip_address::local_ip;
use server::{server_state::ServerState, tracker_endpoint};
use trip_tracker_data_management::DataManager;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Set up application state for use with with_state().
    let (tx, _rx) = broadcast::channel(100);
    let data_manager = DataManager::start().await.unwrap();

    let server_state = Arc::new(ServerState { 
        tx, 
        data_manager, 
        ip_address: local_ip().unwrap()
    });

    let state_clone = server_state.clone();
    tokio::spawn(async move {
        tracker_endpoint::listen(state_clone).await;
    });

    // print base folder
    println!("{:?}", std::env::current_dir().unwrap());

    let app = Router::new()
        .nest_service("/frontend/dist", ServeDir::new("frontend/dist"))
        .fallback_service(ServeFile::new("frontend/dist/index.html"))
        .route("/websocket", get(websocket_handler))
        .route("/tracks", get(get_tracks))
        .with_state(server_state);

    let ip = local_ip().unwrap();
    let listener = tokio::net::TcpListener::bind((ip, 3069))
        .await
        .unwrap();

    tracing::debug!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket(socket, state))
}

async fn websocket(stream: WebSocket, state: Arc<ServerState>) {
    let (mut sender, mut receiver) = stream.split();

    let mut rx = state.tx.subscribe();

    tracing::debug!("socket established");

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            // In any websocket error, break loop.
            if sender.send(Message::text(msg)).await.is_err() {
                break;
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(_text))) = receiver.next().await {
            
        }
    });

    // If any one of the tasks run to completion, we abort the other.
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    };
}

async fn get_tracks(State(state): State<Arc<ServerState>>) -> Bytes {
    let trips = state.data_manager.get_trips().await.unwrap();
    
    if trips.is_empty() {
        return Bytes::from("[]");
    }

    // get latest trip, eg. hihest id:
    let trip = trips.into_iter().max_by_key(|t| t.trip_id).unwrap();
    println!("Latest trip: {:?}", trip);

    let sessions = state.data_manager.get_trip_sessions(trip.trip_id).await.unwrap();

    // count points 
    let points = sessions.iter().map(|s| s.track_points.len()).sum::<usize>();

    // Maybe cache, and no copy? TODO
    Bytes::from_owner(bincode::serialize(&sessions).unwrap())
}