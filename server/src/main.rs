use axum::{
    body::{Body, Bytes}, extract::{ConnectInfo, Path, State}, handler::HandlerWithoutStateExt, http::{uri::Authority, Request, StatusCode, Uri}, middleware::{from_fn_with_state, Next}, response::{IntoResponse, Redirect, Response}, routing::get, BoxError, Router
};
use axum_server::tls_rustls::RustlsConfig;
use chrono::DateTime;
use local_ip_address::local_ip;
use server::{server_state::ServerState, tracker_endpoint};
use tracing::warn;
use trip_tracker_lib::{haversine_distance, track_point::TrackPoint, track_session::TrackSession};
use std::{collections::HashMap, fs::OpenOptions, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, Mutex};
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use data_management::DataManager;
use axum_extra::extract::Host;

#[allow(dead_code)]
#[derive(Clone, Copy)]
struct Ports {
    http: u16,
    https: u16,
}

#[tokio::main]
async fn main() {
    std::fs::create_dir_all("server/log").unwrap();
    let log_file = "server/log/server.log";

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .unwrap();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into())
        )
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(file))
        .init();

    tracing::info!("Starting server...");

    // Set up application state for use with with_state().
    let (tx, _rx) = broadcast::channel(100);
    let data_manager = DataManager::start().await.unwrap();

    let server_state = Arc::new(ServerState {
        tx,
        data_manager,
        ip_address: local_ip().unwrap(),
        ip_load: Mutex::new(HashMap::new()),
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
            "/session_update/{session_id}/{timestamp}",
            get(get_session_update),
        )
        .with_state(server_state.clone())
        .layer(from_fn_with_state(server_state.clone(), ip_middleware));

    // Serve TLS

    let ports = Ports {
        http: 80,
        https: 443,
    };

    tokio::spawn(reset_ip_load(server_state.clone()));

    // configure certificate and private key used by https
    if let Ok(config) = RustlsConfig::from_pem_file(
            PathBuf::from("/etc/letsencrypt/live/tourdelada.dk/fullchain.pem"),
            PathBuf::from("/etc/letsencrypt/live/tourdelada.dk/privkey.pem"),
        ).await {

        tokio::spawn(redirect_http_to_https(ports));

        let ip = local_ip().unwrap();

        axum_server::bind_rustls(SocketAddr::from((ip, ports.https)), config)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .unwrap();

        tracing::debug!("Listening on {}", ip);
    } else {
        warn!("Failed to load certificate. Running in localhost mode");

        let addr = ([127, 0, 0, 1], 80);
        
        axum_server::bind(SocketAddr::from(addr))
            .serve(app.into_make_service())
            .await
            .unwrap();

        tracing::debug!("Listening on localhost");
    }

    tracing::info!("Server running");
}

async fn reset_ip_load(state: Arc<ServerState>) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        state.ip_load.lock().await.clear();
    }
}

// Log and limit access to the server
async fn ip_middleware(State(state): State<Arc<ServerState>>, req: Request<Body>, next: Next) -> Response {
    if let Some(&addr) = req.extensions().get::<ConnectInfo<SocketAddr>>().clone() {
        // Extract path for filtering
        let path = req.uri().path().to_owned();
        
        let count = *state.ip_load.lock().await.get(&addr.ip()).unwrap_or(&0);
        if count > 400 {
            tracing::warn!("IP {} is blocked", addr.ip());
            return StatusCode::TOO_MANY_REQUESTS.into_response()
        }

        let res = next.run(req).await;
        
        if path.starts_with("/frontend/dist/") {
            if path.ends_with(".js") {
                // Filter only frontend requests for .js, as we get 3 for each visit. JS, WASM and CSS
                // Use ConnectInfo extractor for IP address
                if let Err(err) = state.data_manager.record_visit(addr.ip()).await {
                    tracing::error!("Failed to record visit: {err:?}");
                }

                *state.ip_load.lock().await.entry(addr.ip()).or_insert(0) += 1;

                tracing::info!("Visit   from: {}", addr.ip())
            }
        } else {
            //tracing::debug!("Request from: {} with result {} for {}", addr.ip(), res.status(), path);
        };

        return res;
    }

    /*tracing::error!("Failed to get address of request");
    StatusCode::BAD_REQUEST.into_response()*/

    next.run(req).await
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
        tracing::error!("Failed to get trip");
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn get_session(
    State(state): State<Arc<ServerState>>,
    Path(session_id): Path<i64>,
) -> Response {
    let session = state.data_manager.get_session(session_id).await;
    match session {
        Ok(session) => {
            Bytes::from_owner(bincode::serialize(&filter_anomalies(session)).unwrap()).into_response()
        },
        Err(err) => {
            tracing::error!("Failed to get session {}: {:?}", session_id, err);
            StatusCode::NOT_FOUND.into_response()
        },
    }
}

pub fn filter_anomalies(mut session: TrackSession) -> TrackSession {
    let mut filtered_points = Vec::new();
    // Filter out points that are very far from its neighbors, and points that go "back" in time.
 
    if session.track_points.len() < 3 {
        return session;
    }

   // session.track_points.sort_by_key(|p| p.timestamp);

    let mut prev_point = &session.track_points[0];
    for i in 1..session.track_points.len() - 1 {
        let curr_point = &session.track_points[i];
        let next_point = &session.track_points[i + 1];

        // Calculate the distance between the two points
        let dist_to_prev = haversine_distance((prev_point.latitude, prev_point.longitude), (curr_point.latitude, curr_point.longitude));
        let dist_to_next = haversine_distance((curr_point.latitude, curr_point.longitude), (next_point.latitude, next_point.longitude));
        let neighbor_dist = haversine_distance((prev_point.latitude, prev_point.longitude), (next_point.latitude, next_point.longitude));

        if dist_to_prev + dist_to_next > neighbor_dist * 5. {
            continue;
        }

        if dist_to_prev > 5.0 {
            continue;           
        }

        if filtered_points.iter().any(|p: &TrackPoint| p.latitude == curr_point.latitude && p.longitude == curr_point.longitude) {
            continue;
        }

        filtered_points.push(curr_point.clone());
        prev_point = curr_point;
    }
    

    session.track_points = filtered_points.into_iter().step_by(6).collect();

    session
}

async fn get_session_update(
    State(state): State<Arc<ServerState>>,
    Path((session_id, timestamp)): Path<(i64, i64)>,
) -> Response {
    let update = state
        .data_manager
        .get_session_update(session_id, DateTime::from_timestamp(timestamp, 0).unwrap().to_utc())
        .await;

    if let Ok(update) = update {
        // Maybe cache, and no copy? TODO
        Bytes::from_owner(bincode::serialize(&update).unwrap()).into_response()
    } else {
        tracing::error!("Failed to get session update");
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn get_trip_session_ids(
    State(state): State<Arc<ServerState>>,
    Path(trip_id): Path<i64>,
) -> Response {
    let ids = state.data_manager.get_nonhidden_trip_session_ids(trip_id).await;

    if let Ok(ids) = ids {
        // Maybe cache, and no copy? TODO
        Bytes::from_owner(bincode::serialize(&ids).unwrap()).into_response()
    } else {
        tracing::error!("Failed to get trip session ids");
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
    tracing::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, redirect.into_make_service())
        .await
        .unwrap();
}