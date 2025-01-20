mod core;

use axum::{
    body::Bytes, extract::{
        ws::{Message, WebSocket, WebSocketUpgrade}, FromRef, State
    }, response::IntoResponse, routing::get, Json, Router
};
use futures::{sink::SinkExt, stream::StreamExt};
use base64::prelude::*;
use sqlx::Sqlite;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tower_http::services::{ServeDir, ServeFile};

struct AppState {
    // Channel used to send messages to all connected clients.
    tx: broadcast::Sender<String>,
    db: core::db::TripDatabase<Sqlite>,
}

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
    let db = core::db::TripDatabase::connect().await;

    let app_state = AppState { tx, db };

    // print base folder
    println!("{:?}", std::env::current_dir().unwrap());

    let app = Router::new()
        .nest_service("/frontend/dist", ServeDir::new("frontend/dist"))
        .fallback_service(ServeFile::new("frontend/dist/index.html"))
        .route("/websocket", get(websocket_handler))
        .route("/tracks", get(get_tracks))
        .with_state(app_state.into());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3069")
        .await
        .unwrap();

    tracing::debug!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| websocket(socket, state))
}

async fn websocket(stream: WebSocket, state: Arc<AppState>) {
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

async fn get_tracks(State(state): State<Arc<AppState>>) -> Bytes {
    let trips = state.db.get_trips().await;
    // get latest trip, eg. hihest id:
    let trip = trips.iter().max_by_key(|t| t.trip_id).unwrap();

    let sessions = state.db.get_trip_sessions(trip.trip_id).await;

    // Maybe cache, and no copy? TODO
    Bytes::copy_from_slice(&bincode::serialize(&sessions).unwrap())
}