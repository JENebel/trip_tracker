
use const_format::concatcp;

pub mod database;
mod gpx_util;
mod tsf_util;
pub mod buffer;
mod data_manager;
pub mod geonames;

pub use data_manager::*;

pub const DATA_DIR: &str = "data/";
pub const DATABASE_PATH: &str = concatcp!(DATA_DIR, "database.db");
pub const BUFFER_FILE_DIR: &str = concatcp!(DATA_DIR, "buffer_files");
pub const COUNTRY_FILE: &str = concatcp!(DATA_DIR, "countries.geojson");

#[derive(Debug)]
pub enum DataManagerError {
    Database(String),
    BufferManager(String),
}