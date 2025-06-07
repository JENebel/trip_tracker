#![cfg_attr(all(not(feature = "std"), not(test)), no_std)]
#![feature(f16)]

pub mod track_point;
pub mod comms;

#[cfg(feature = "std")]
pub mod traffic;
#[cfg(feature = "std")]
pub mod track_session;
#[cfg(feature = "std")]
pub mod trip;

#[cfg(feature = "std")]
pub fn haversine_distance(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    const R: f64 = 6372.8; // Radius of the earth in km

    let d_lat = (p2.0 - p1.0).to_radians();
    let d_lon = (p2.1 - p1.1).to_radians();
    let lat1 = p1.0.to_radians();
    let lat2 = p2.0.to_radians();

    let a = f64::sin(d_lat / 2.).powi(2)
        + f64::cos(lat1) * f64::cos(lat2) * f64::sin(d_lon / 2.).powi(2);
    let c = 2. * f64::asin(f64::sqrt(a));

    R * c
}