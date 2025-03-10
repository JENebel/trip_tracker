pub const SIGNATURE_SIZE: usize = 16; // bytes

pub trait MacProvider {
    /// Performs a HMAC-SHA256 signature of the data using the token as the key.
    /// So SHA256(data || tooken). This should be safe (!?) and prevent extension attacks according to Wikipedia
    fn sign(&mut self, data: &[u8], token: &[u8]) -> [u8; SIGNATURE_SIZE];

    fn verify(&mut self, data: &[u8], signature: &[u8], key: &[u8]) -> bool {
        Self::sign(self, data, key).as_slice() == signature
    }
}

#[derive(Clone, Debug)]
pub enum CommsError {
    DecodeError,
    EncodeError,
    WrongSignature,
}

pub enum HandshakeMessage {
    FreshSession {
        trip_id: i64,
        timestamp: i64,
    },
    Reconnect {
        trip_id: i64,
        session_id: i64,
    },
}

impl HandshakeMessage {
    pub fn new_fresh(trip_id: i64, timestamp: i64) -> Self {
        Self::FreshSession {
            trip_id,
            timestamp,
        }
    }

    pub fn new_reconnect(trip_id: i64, session_id: i64) -> Self {
        Self::Reconnect {
            trip_id,
            session_id,
        }
    }

    pub fn trip_id(&self) -> i64 {
        match self {
            Self::FreshSession { trip_id, .. } => *trip_id,
            Self::Reconnect { trip_id, .. } => *trip_id,
        }
    }

    pub fn session_id(&self) -> i64 {
        match self {
            Self::FreshSession { timestamp, .. } => *timestamp,
            Self::Reconnect { session_id, .. } => *session_id,
        }
    }

    pub fn is_fresh_session(&self) -> bool {
        match self {
            Self::FreshSession { .. } => true,
            Self::Reconnect { .. } => false,
        }
    }
}

impl HandshakeMessage {
    pub fn serialize(&self) -> [u8; 17] {
        let mut data = [0; 17];

        match self {
            Self::FreshSession { trip_id, timestamp } => {
                data[0] = 0;
                data[1..9].copy_from_slice(&trip_id.to_be_bytes());
                data[9..17].copy_from_slice(&timestamp.to_be_bytes());
            },
            Self::Reconnect { trip_id, session_id } => {
                data[0] = 1;
                data[1..9].copy_from_slice(&trip_id.to_be_bytes());
                data[9..17].copy_from_slice(&session_id.to_be_bytes());
            },
        }

        data
    }

    pub fn deserialize(data: &[u8; 17]) -> Result<Self, CommsError> {
        let message_type = data[0];
        let trip_id = i64::from_be_bytes(data[1..9].try_into().unwrap());
        let session_id_or_timestamp = i64::from_be_bytes(data[9..17].try_into().unwrap());
        
        match message_type {
            0 => Ok(Self::new_fresh(trip_id, session_id_or_timestamp)),
            1 => Ok(Self::new_reconnect(trip_id, session_id_or_timestamp)),
            _ => Err(CommsError::DecodeError),
        }
    }
}