use std::{net::{IpAddr, SocketAddr}, sync::Arc};

use chrono::DateTime;
use sha2::{Sha256, Digest};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}, sync::Mutex};
use trip_tracker_lib::{comms::{MacProvider, SIGNATURE_SIZE}, track_point::TrackPoint};
use bimap::BiMap;

use crate::server_state::ServerState;

pub struct Connection {
    pub token: String,
    pub trip_id: i64,
}

#[derive(Clone)]
pub struct EndpointState {
    pub connected_sessions: Arc<Mutex<BiMap<IpAddr, i64>>>,
    pub banned_ips: Arc<Mutex<Vec<IpAddr>>>,
}

pub async fn listen(server_state: Arc<ServerState>) {
    let ip: IpAddr = server_state.ip_address;
    let listener = TcpListener::bind((ip, 3169)).await.unwrap();

    let endpoint_state = EndpointState {
        connected_sessions: Arc::new(Mutex::new(BiMap::new())),
        banned_ips: Arc::new(Mutex::new(Vec::new())),
    };

    println!("Listening on {}", ip);
    loop {
        let (stream, addr) = listener.accept().await.unwrap();

        println!("New connection from {}", addr);

        if endpoint_state.banned_ips.lock().await.contains(&addr.ip()) {
            // Ignore banned IP addresses.
            continue;
        }

        let endpoint_state = endpoint_state.clone();
        let server_state = server_state.clone();
        tokio::spawn(async move {
            let res = handle_connection(stream, addr.clone(), endpoint_state.clone(), server_state).await;
            endpoint_state.connected_sessions.lock().await.remove_by_left(&addr.ip());
            println!("Connection from {} ended with result: {:?}", addr, res);
        });
    }
}

pub async fn handle_connection(mut stream: TcpStream, addr: SocketAddr, endpoint_state: EndpointState, server_state: Arc<ServerState>) -> Result<(), anyhow::Error> {
    // First we do the handshake:
    // 1. Send 16 random bytes to the tracker.
    // 2. Receive the same 16 bytes from the tracker + a trip id + [session_id OR new session with i64 timestamp] + a signature
    // 2.5 If resuming a session, the section is [0, session_id(i64)], if new session, the section is [1, timestamp(i64)]
    // 3. Check if the signature is correct for the given trip id.
    // 4. Start listening to updates from the tracker.

    let random_bytes: [u8; 16] = rand::random();
    stream.write_all(&random_bytes).await?; // TODO unwrap

    let mut buf = [0; 16 + 8 + 5 + SIGNATURE_SIZE];
    stream.read_exact(&mut buf).await?; // TODO timeout

    let data = &buf[0..29];
    let trip_id = i64::from_be_bytes(data[16..24].try_into().unwrap()); // Safe unwrap
    let resume_or_new = data[24];
    let session_id_or_timestamp = i64::from_be_bytes(data[25..].try_into().unwrap()); // Safe unwrap
    let signature = buf[24..].try_into().unwrap(); // Safe unwrap

    if data != random_bytes {
        // The tracker didn't send the correct data.
        println!("Tracker didn't send correct data");
        return Ok(());
    }

    let trip = server_state.data_manager.get_trip(trip_id).await.unwrap(); // TODO unwrap
    let key = trip.api_token.as_bytes();

    if !ServerMacProvider::verify(data, key, signature) {
        // The signature is incorrect.
        println!("Signature is incorrect");
        return Ok(());
    }

    // Authenticated! Now we can start the session.

    let session_id = if resume_or_new == 1 {
        // New session id should be sent to the tracker.
        let ts = DateTime::from_timestamp(session_id_or_timestamp, 0).unwrap();
        let session = server_state.data_manager.register_new_session(trip_id, format!("Unnamed {}", ts.date_naive()), "".into()).await.unwrap(); // TODO unwrap
        stream.write_all(&session.session_id.to_be_bytes()).await.unwrap(); // TODO unwrap
        session.session_id
    } else {
        // Check that noone else is sending on this session id.
        if endpoint_state.connected_sessions.lock().await.contains_right(&session_id_or_timestamp) {
            // Already a session with this id.
            println!("Session id already has active connection");
            return Ok(());
        }
        session_id_or_timestamp
    };

    endpoint_state.connected_sessions.lock().await.insert(addr.ip(), session_id);

    // Now we can start listening to the tracker sending data.
    let mut buffer = [0; 256 * 15  + 16]; // Max package size. ~4 minutes worth of data
    loop {
        let header = stream.read_u8().await.unwrap(); // TODO timeout

        if header == 0 {
            // Terminates the session. TODO Authentication!? Maybe sign the sessionID? That should be enough.
            server_state.data_manager.end_session(session_id).await.unwrap(); // TODO unwrap
            break;
        }

        stream.read_exact(&mut buffer[..header as usize * 15 + 16]).await.unwrap(); // TODO timeout
        let data = &buffer[..header as usize * 15];
        let signature = &buffer[header as usize * 15..];

        let trip = server_state.data_manager.get_trip(trip_id).await.unwrap(); // TODO unwrap
        if !ServerMacProvider::verify(data, signature, trip.api_token.as_bytes()) {
            println!("Signature is incorrect!");
            continue;
        }

        // Message authenticated, now we can store the data.

        let session = server_state.data_manager.get_session(session_id).await.unwrap(); // TODO unwrap
        let data_manager = &server_state.data_manager;
        for i in 0..header as usize {
            let point = TrackPoint::from_bytes(&data[i * 15..i * 15 + 15], session.timestamp);
            data_manager.append_gps_point(session.session_id, point).await.unwrap(); // TODO unwrap
        }
    }

    Ok(())
}

pub struct ServerMacProvider {  }

impl MacProvider for ServerMacProvider {
    fn sign(data: &[u8], token: &[u8]) -> [u8; SIGNATURE_SIZE] {
        let mut hasher = Sha256::new();

        hasher.update(data);
        hasher.update(token);

        let result = hasher.finalize();

        result[..SIGNATURE_SIZE].try_into().unwrap()
    }
}