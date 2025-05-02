use axum::{
    body::Bytes, extract::{Path, State}, handler::HandlerWithoutStateExt, http::{uri::Authority, StatusCode, Uri}, response::{IntoResponse, Redirect, Response}, routing::get, BoxError, Router
};
use axum_server::tls_rustls::RustlsConfig;
use local_ip_address::local_ip;
use server::{server_state::ServerState, tracker_endpoint};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::broadcast;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use trip_tracker_data_management::DataManager;
use axum_extra::extract::Host;

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct Ports {
    http: u16,
    https: u16,
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
    let data_manager = DataManager::start().await.unwrap();

    let server_state = Arc::new(ServerState {
        tx,
        data_manager,
        ip_address: local_ip().unwrap(),
    });

    let state_clone = server_state.clone();
    tokio::spawn(async move {
        tracker_endpoint::listen(state_clone).await;
    });

    let app = Router::new()
        .nest_service("/frontend/dist", ServeDir::new("frontend/dist"))
        .fallback_service(ServeFile::new("frontend/dist/index.html"))
        .route("/trip_ids", get(get_trip_ids))
        .route("/trip/{trip_id}", get(get_trip))
        .route("/session_ids/{trip_id}", get(get_trip_session_ids))
        .route("/session/{session_id}", get(get_session))
        .route(
            "/session_update/{session_id}/{current_points}",
            get(get_session_update),
        )
        .with_state(server_state);

    // Serve TLS

    let ports = Ports {
        http: 80,
        https: 443,
    };

    tokio::spawn(redirect_http_to_https(ports));

    // configure certificate and private key used by https
    let config = RustlsConfig::from_pem_file(
        PathBuf::from("/etc/letsencrypt/live/joachimen.dk/fullchain.pem"),
        PathBuf::from("/etc/letsencrypt/live/joachimen.dk/privkey.pem"),
    )
    .await
    .unwrap();

    let ip = local_ip().unwrap();
    tracing::debug!("listening on {}", ip);

    axum_server::bind_rustls(SocketAddr::from((ip, ports.https)), config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn get_trip_ids(State(state): State<Arc<ServerState>>) -> Response {
    let trips = state.data_manager.get_trips().await.unwrap();
    let ids = trips.iter().map(|trip| trip.trip_id).collect::<Vec<i64>>();
    Bytes::from_owner(bincode::serialize(&ids).unwrap()).into_response()
}

async fn get_trip(State(state): State<Arc<ServerState>>, Path(trip_id): Path<i64>) -> Response {
    let trip = state.data_manager.get_trip(trip_id).await;

    if let Ok(mut trip) = trip {
        trip.api_token = String::new(); // Do not send API token. This is not pretty XD

        // Maybe cache, and no copy? TODO
        Bytes::from_owner(bincode::serialize(&trip).unwrap()).into_response()
    } else {
        println!("Failed to get trip");
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn get_session(
    State(state): State<Arc<ServerState>>,
    Path(session_id): Path<i64>,
) -> Response {
    let session = state.data_manager.get_session(session_id).await;
    match session {
        Ok(session) => Bytes::from_owner(bincode::serialize(&session).unwrap()).into_response(),
        Err(err) => {
            println!("Failed to get session {}: {:?}", session_id, err);
            StatusCode::NOT_FOUND.into_response()
        },
    }
}

async fn get_session_update(
    State(state): State<Arc<ServerState>>,
    Path(session_id): Path<i64>,
    Path(current_points): Path<usize>,
) -> Response {
    let update = state
        .data_manager
        .get_session_update(session_id, current_points)
        .await;

    if let Ok(update) = update {
        // Maybe cache, and no copy? TODO
        Bytes::from_owner(bincode::serialize(&update).unwrap()).into_response()
    } else {
        println!("Failed to get session update");
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn get_trip_session_ids(
    State(state): State<Arc<ServerState>>,
    Path(trip_id): Path<i64>,
) -> Response {
    let ids = state.data_manager.get_trip_session_ids(trip_id).await;

    if let Ok(ids) = ids {
        // Maybe cache, and no copy? TODO
        Bytes::from_owner(bincode::serialize(&ids).unwrap()).into_response()
    } else {
        println!("Failed to get trip session ids");
        StatusCode::NOT_FOUND.into_response()
    }
}


#[allow(dead_code)]
async fn redirect_http_to_https(ports: Ports) {
    fn make_https(host: &str, uri: Uri, https_port: u16) -> Result<Uri, BoxError> {
        let mut parts = uri.into_parts();

        parts.scheme = Some(axum::http::uri::Scheme::HTTPS);

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some("/".parse().unwrap());
        }

        let authority: Authority = host.parse()?;
        let bare_host = match authority.port() {
            Some(port_struct) => authority
                .as_str()
                .strip_suffix(port_struct.as_str())
                .unwrap()
                .strip_suffix(':')
                .unwrap(), // if authority.port() is Some(port) then we can be sure authority ends with :{port}
            None => authority.as_str(),
        };

        parts.authority = Some(format!("{bare_host}:{https_port}").parse()?);

        Ok(Uri::from_parts(parts)?)
    }

    let redirect = move |Host(host): Host, uri: Uri| async move {
        match make_https(&host, uri, ports.https) {
            Ok(uri) => Ok(Redirect::permanent(&uri.to_string())),
            Err(error) => {
                tracing::warn!(%error, "failed to convert URI to HTTPS");
                Err(StatusCode::BAD_REQUEST)
            }
        }
    };

    let addr = SocketAddr::from((local_ip().unwrap(), ports.http));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, redirect.into_make_service())
        .await
        .unwrap();
}