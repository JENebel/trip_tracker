use core::fmt::Display;

use chrono::{DateTime, Utc};

pub const ENCODED_LENGTH: usize = 15;

// Todo, move to tsf_util?
#[cfg(feature = "std")]
pub fn parse_tsf(bytes: &[u8]) -> Result<(Vec<TrackPoint>, DateTime<Utc>), &'static str> {
    let mut buffer = [0; ENCODED_LENGTH];
    let timestamp = i64::from_be_bytes(bytes[..8].try_into().map_err(|_| "Less than 8 bytes")?);
    let start_time = DateTime::from_timestamp(timestamp, 0).unwrap().to_utc();
    let mut track_points = Vec::new();
    let mut i = 8;
    while i < bytes.len() {
        buffer.copy_from_slice(&bytes[i..i + ENCODED_LENGTH]);
        let tp = TrackPoint::from_bytes(&buffer, start_time);
        track_points.push(tp);
        i += ENCODED_LENGTH;
    }
    Ok((track_points, start_time))
}

// Todo, move to tsf_util?
#[cfg(feature = "std")]
pub fn write_tsf(start_time: DateTime<Utc>, track_points: &[TrackPoint]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&start_time.timestamp().to_be_bytes());
    for tp in track_points {
        bytes.extend_from_slice(&tp.to_bytes(start_time));
    }
    bytes
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackPoint {
    pub timestamp: DateTime<Utc>, // 3 byte timestamp (seconds since session start). Up to ~200 days
    pub latitude: f64,            // 31 bits. 29 and 30 its would still be 3.7 cm precision, and leave 5 bits for other stuff
    pub longitude: f64,           // 32 bits 63 bits/8 bytes for both lat and lon. 1 bit extra!
    pub altitude: f32,            // 2 bytes when compressed to u16
    pub speed_kph: f32,           // 2 bytes when compressed to u16
    /// HDOP was < 1.0, and the fix was good
    pub good_precision: bool,     // 1 bit - pack into position fields ^^^
}
// 15 bytes total, maybe 5 byte (32 bit) MAC?

impl TrackPoint {
    pub fn new(timestamp: DateTime<Utc>, latitude: f64, longitude: f64, altitude: f32, speed_kph: f32, good_precision: bool) -> Self {
        Self {
            timestamp,
            latitude,
            longitude,
            altitude,
            speed_kph,
            good_precision,
        }
    }
}

impl Display for TrackPoint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}, ({}, {}), {} m, {} km/h, good: {}", self.timestamp, self.latitude, self.longitude, self.altitude, self.speed_kph, self.good_precision)
    }
}

impl TrackPoint {
    pub fn to_bytes(&self, session_start: DateTime<Utc>) -> [u8; ENCODED_LENGTH] {
        let mut bytes = [0; ENCODED_LENGTH];
        bytes[..3].copy_from_slice(&((self.timestamp - session_start)).num_seconds().to_be_bytes()[5..]);
        //println!("{:?}", &((self.timestamp - session_start)).num_seconds().to_be_bytes()[5..]);
        let lat_lon = encode_lat_lon_precision(self.latitude, self.longitude, self.good_precision);
        bytes[3..11].copy_from_slice(&lat_lon.to_be_bytes());
        bytes[11..13].copy_from_slice(&encode_alt(self.altitude).to_be_bytes());
        bytes[13..].copy_from_slice(&encode_speed(self.speed_kph).to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8], session_start: DateTime<Utc>) -> TrackPoint {
        let timestamp = i64::from_be_bytes([0, 0, 0, 0, 0, bytes[0], bytes[1], bytes[2]]);
        let lat_lon = u64::from_be_bytes(bytes[3..11].try_into().unwrap());
        let altitude = decode_alt(u16::from_be_bytes(bytes[11..13].try_into().unwrap()));
        let speed = decode_speed(u16::from_be_bytes(bytes[13..].try_into().unwrap()));
        let (latitude, longitude, good_precision) = decode_lat_lon_precision(lat_lon);

        let datetime = session_start + chrono::Duration::seconds(timestamp);

        TrackPoint {
            timestamp: datetime,
            latitude,
            longitude,
            altitude,
            speed_kph: speed,
            good_precision,
        }
    }
}

const MAX_LAT_U32: u32 = 2u32.pow(31) - 1;

fn encode_lat(latitude: f64) -> u32 {
    (latitude * (MAX_LAT_U32 as f64 / 180.0) + (MAX_LAT_U32 as f64 / 2.0)) as u32
}

fn decode_lat(encoded: u32) -> f64 {
    (encoded as f64 - (MAX_LAT_U32 as f64 / 2.0)) / (MAX_LAT_U32 as f64 / 180.0)
}

fn encode_lon(latitude: f64) -> u32 {
    (latitude * (u32::MAX as f64 / 360.0) + (u32::MAX as f64 / 2.0)) as u32
}

fn decode_lon(encoded: u32) -> f64 {
    (encoded as f64 - (u32::MAX as f64 / 2.0)) / (u32::MAX as f64 / 360.0)
}

// Altitude
const ALT_MIN: f32 = -10.0;
const ALT_MAX: f32 = 6_000.0;
const MAX_ALT_ERROR: f32 = (ALT_MAX - ALT_MIN) / (u16::MAX as f32);

fn encode_alt(altitude: f32) -> u16 {
    if altitude < ALT_MIN {
        return u16::MIN;
    }
    if altitude > ALT_MAX {
        return u16::MAX;
    }
    ((altitude - ALT_MIN) / (ALT_MAX - ALT_MIN) * (u16::MAX as f32)) as u16
}

fn decode_alt(encoded: u16) -> f32 {
    ALT_MIN + (encoded as f32) / (u16::MAX as f32) * (ALT_MAX - ALT_MIN) + MAX_ALT_ERROR / 2.
}

// Speed
const SPEED_MIN: f32 = 0.0;
const SPEED_MAX: f32 = 500.0;
const MAX_SPEED_ERROR: f32 = (SPEED_MAX - SPEED_MIN) / (u16::MAX as f32);

fn encode_speed(speed: f32) -> u16 {
    if speed > SPEED_MAX {
        return u16::MAX;
    }
    ((speed - SPEED_MIN) / (SPEED_MAX - SPEED_MIN) * (u16::MAX as f32)) as u16
}

fn decode_speed(encoded: u16) -> f32 {
    SPEED_MIN + (encoded as f32) / (u16::MAX as f32) * (SPEED_MAX - SPEED_MIN) + MAX_SPEED_ERROR / 2.
}

fn encode_lat_lon_precision(lat: f64, lon: f64, precise: bool) -> u64 {
    let lat = encode_lat(lat) & 0x7FFFFFFF;
    let lon = encode_lon(lon);
    // Shift into u64, and take the first 55 bits
    let mut lat_lon = ((lat as u64) << 32) | (lon as u64);
    
    // Set the 56th bit to the precision flag
    if precise {
        lat_lon |= 0x8000000000000000;
    }

    lat_lon
}

fn decode_lat_lon_precision(encoded: u64) -> (f64, f64, bool) {
    let lat = decode_lat((encoded >> 32) as u32 & 0x7FFFFFFF);
    let lon = decode_lon(encoded as u32);
    let precise = (encoded & 0x8000000000000000) != 0;

    (lat, lon, precise)
}

#[test]
fn test() {
    let tp = TrackPoint::new(DateTime::from_timestamp_millis(1233456).unwrap().to_utc(), -90., 180., 10.0, 50.0, true);
    let llp = encode_lat_lon_precision(tp.latitude, tp.longitude, tp.good_precision);

    println!("{llp:064b}");

    println!("{:?}", decode_lat_lon_precision(llp));
}

#[test]
fn encode_decode_test() {
    let start_time = DateTime::from_timestamp(0, 0).unwrap().to_utc();
    let tp = TrackPoint::new(DateTime::from_timestamp(3, 0).unwrap().to_utc(), -90., 180., 10.0, 50.0, true);
    let bytes = tp.to_bytes(start_time);
    let tp2 = TrackPoint::from_bytes(&bytes, start_time);

    println!("{:?}", tp2);
}

#[test]
#[cfg(feature = "std")]
fn write_parse_test() {
    let start_time = DateTime::from_timestamp(0, 0).unwrap().to_utc();
    let track_points = vec![
        TrackPoint::new(DateTime::from_timestamp(3, 0).unwrap().to_utc(), -90., 180., 10.0, 50.0, true),
        TrackPoint::new(DateTime::from_timestamp(4, 0).unwrap().to_utc(), -90., 180., 10.0, 50.0, true),
        TrackPoint::new(DateTime::from_timestamp(5, 0).unwrap().to_utc(), -90., 180., 10.0, 50.0, true),
    ];
    let bytes = write_tsf(start_time, &track_points);
    let (track_points2, start_time2) = parse_tsf(&bytes).unwrap();

    println!("{:?}", track_points2);
}