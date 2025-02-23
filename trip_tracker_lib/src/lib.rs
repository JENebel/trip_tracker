#![cfg_attr(all(not(feature = "std"), not(test)), no_std)]
#![feature(f16)]

pub mod track_point;

#[cfg(feature = "std")]
pub mod track_session;
#[cfg(feature = "std")]
pub mod trip;
#[cfg(feature = "std")]
pub mod user;