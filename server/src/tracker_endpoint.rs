use core::time;
use std::{net::{IpAddr, SocketAddr}, sync::Arc};

use chrono::DateTime;
use sha2::{Sha256, Digest};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}, sync::Mutex};
use trip_tracker_lib::{comms::{HandshakeMessage, MacProvider, SIGNATURE_SIZE}, track_point::{TrackPoint, ENCODED_LENGTH}};
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
    // 2. Receive from the tracker: trip id + [session_id OR new session with i64 timestamp] + a signature
    // 2.5 If resuming a session, the section is [0, session_id(i64)], if new session, the section is [1, timestamp(i64)]
    // 3. Check if the signature is correct for the given trip id.
    // 4. Start listening to updates from the tracker.

    let random_bytes: [u8; 16] = rand::random();
    println!("Sending random bytes: {:?}", random_bytes);
    stream.write_all(&random_bytes).await?; // TODO unwrap

    let mut buf = [0; 8 + 1 + 8 + SIGNATURE_SIZE];
    stream.read_exact(&mut buf).await?; // TODO timeout

    let handshake_bytes = &buf[..17]; // Safe unwrap
    let handshake_message = HandshakeMessage::deserialize(handshake_bytes.try_into().unwrap()).unwrap(); // TODO unwrap
    let signature = buf[17..].try_into().unwrap(); // Safe unwrap

    let mut to_sign = [0; 16 + 1 + 8 + 8];
    to_sign[..16].copy_from_slice(&random_bytes);
    to_sign[16..].copy_from_slice(&handshake_bytes);

    let trip = server_state.data_manager.get_trip(handshake_message.trip_id()).await.unwrap(); // TODO unwrap
    let key = hex::decode(trip.api_token).unwrap(); // TODO unwrap

    println!("Verifying signature {:?}", &signature);
    println!("Data: {:?}", &to_sign);
    println!("Key: {:?}", &key);
    println!("Expected signature {:?}", (ServerMacProvider{}).sign(&to_sign, &key));

    if !(ServerMacProvider{}).verify(&to_sign, signature, &key) {
        // The signature is incorrect.
        println!("Signature is incorrect");
        return Ok(());
    }

    // Authenticated! Now we can start the session.

    let (session_id, timestamp) =  match handshake_message {
        HandshakeMessage::FreshSession { trip_id, timestamp } => {
            // New session id should be sent to the tracker.
            let ts = DateTime::from_timestamp(timestamp, 0).unwrap();
            let session = server_state.data_manager.register_new_live_session(trip_id, format!("Unnamed {}", ts.date_naive()), "".into()).await.unwrap(); // TODO unwrap
            stream.write_all(&session.session_id.to_be_bytes()).await.unwrap(); // TODO unwrap
            (session.session_id, DateTime::from_timestamp(timestamp, 0).unwrap())
        },
        HandshakeMessage::Reconnect { trip_id: _, session_id } => {
            // Check that noone else is sending on this session id.
            if endpoint_state.connected_sessions.lock().await.contains_right(&session_id) {
                // Already a session with this id.
                println!("Session id already has active connection");
                return Ok(());
            }
            let session = server_state.data_manager.get_session(session_id).await.unwrap(); // TODO unwrap
            (session_id, session.timestamp)
        },
    };

    endpoint_state.connected_sessions.lock().await.insert(addr.ip(), session_id);

    // Now we can start listening to the tracker sending data.
    let mut buffer = [0; 1 + 256 * ENCODED_LENGTH  + SIGNATURE_SIZE]; // Max package size. ~4 minutes worth of data

    println!("All good! Starting to listen to data");
    loop {
        stream.read_exact(&mut buffer[..1]).await.unwrap(); // TODO timeout
        let header = buffer[0];
        println!("Header: {}", header);

        if header == 0 {
            // Do some sort of handshake here.
            // Terminates the session. TODO Authentication!? Maybe sign the sessionID? That should be enough.
            // let _ = server_state.data_manager.end_session(session_id).await; // TODO unwrap
            // println!("Session ended");
            // break;
            panic!("Empty header!");
        }

        let bytes_to_read = header as usize * ENCODED_LENGTH + SIGNATURE_SIZE;
        println!("Reading {} bytes...", bytes_to_read);

        stream.read_exact(&mut buffer[1..bytes_to_read + 1]).await.unwrap(); // TODO timeout
        let data = &buffer[..bytes_to_read - 16 + 1];
        println!("Data: {:?}", data);
        let signature = &buffer[bytes_to_read - 16 + 1..bytes_to_read + 1];

        println!("Expected signature {:?}", (ServerMacProvider{}).sign(&data, &key));
        println!("Actual signature: {:?}", signature);

        if !(ServerMacProvider{}).verify(data, signature, &key) {
            println!("Signature is incorrect!");
            break;
        }

        // Message authenticated, now we can store the data.

        let data_manager = &server_state.data_manager;
        for i in 0..header as usize {
            let point = TrackPoint::from_bytes(&data[i * 15 + 1..i * 15 + 15 + 1], timestamp);
            data_manager.append_gps_point(session_id, point).await.unwrap(); // TODO unwrap
        }

        println!("Received {} points", header);
    }

    Ok(())
}

pub struct ServerMacProvider {  }

impl MacProvider for ServerMacProvider {
    fn sign(&mut self, data: &[u8], token: &[u8]) -> [u8; SIGNATURE_SIZE] {
        let mut hasher = Sha256::new();

        hasher.update(data);
        hasher.update(token);

        let result = hasher.finalize();

        result[..SIGNATURE_SIZE].try_into().unwrap()
    }
}