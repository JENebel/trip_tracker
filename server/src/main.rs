use axum::{
    body::Bytes, extract::{
        ws::{Message, WebSocket, WebSocketUpgrade}, Path, State
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
        .route("/trips", get(get_trips))
        .route("/sessions/{trip_id}", get(get_sessions))
        .route("/session/{session_id}", get(get_session))
        .route("/session_update/{session_id}/{current_points}", get(get_session_update))
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

async fn get_trips(State(state): State<Arc<ServerState>>) -> Bytes {
    let trips = state.data_manager.get_trips().await.unwrap();
    
    if trips.is_empty() {
        return Bytes::from("[]");
    }

    // Maybe cache, and no copy? TODO
    Bytes::from_owner(bincode::serialize(&trips).unwrap())
}

async fn get_sessions(State(state): State<Arc<ServerState>>, Path(trip_id): Path<i64>) -> Bytes {
    let trip = state.data_manager.get_trip(trip_id).await;
    let Ok(trip) = trip else {
        return Bytes::from("[]");
    };

    let sessions = state.data_manager.get_trip_sessions(trip.trip_id).await.unwrap();

    // Maybe cache, and no copy? TODO
    Bytes::from_owner(bincode::serialize(&sessions).unwrap())
}

async fn get_session(State(state): State<Arc<ServerState>>, Path(session_id): Path<i64>) -> Bytes {
    let session = state.data_manager.get_session(session_id).await;
    
    if let Ok(session) = session {
        // Maybe cache, and no copy? TODO
        Bytes::from_owner(bincode::serialize(&session).unwrap())
    } else {
        Bytes::from("[]")
    }
}

async fn get_session_update(State(state): State<Arc<ServerState>>, Path(session_id): Path<i64>, Path(current_points): Path<usize>) -> Bytes {
    let update = state.data_manager.get_session_update(session_id, current_points).await;
    
    if let Ok(update) = update {
        // Maybe cache, and no copy? TODO
        Bytes::from_owner(bincode::serialize(&update).unwrap())
    } else {
        Bytes::from("[]")
    }
}