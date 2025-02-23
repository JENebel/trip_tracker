
use alloc::string::ToString;
use embassy_sync::once_lock::OnceLock;

use crate::ExclusiveService;

use super::StorageService;

pub static LOGGER: OnceLock<Logger> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct Logger {
    pub storage_service: ExclusiveService<StorageService>,
}

impl Logger {
    pub fn init(storage_service: ExclusiveService<StorageService>) {
        LOGGER.init(Logger {
            storage_service,
        }).unwrap();
    }

    async fn log() {
        let Some(message) = format_args!("$($arg)+").as_str() else {
            esp_println::println!("DEBUG: Empty log message");
            return;
        };

        let file = file!();
        let line = line!();
        let column = column!();
        let prefix = format_args!("INFO: [{}:{}:{}][time] ", file, line, column).to_string();

        let Some(logger) = crate::log::LOGGER.try_get() else {
            esp_println::println!("Logger not initialized");
            return;
        };
        
        let mut storage_service = logger.storage_service.lock().await;
        storage_service.append_to_session_log(prefix.as_bytes());
        storage_service.append_to_session_log(message.as_bytes());
        storage_service.append_to_session_log("\n".as_bytes());
    }
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        extern crate alloc;
        use alloc::string::ToString;

        let message = format_args!($($arg)+).to_string();
        if message.is_empty() {
            esp_println::println!("DEBUG: Empty log message");
            return;
        };

        let file = file!();
        let line = line!();
        let column = column!();
        let prefix = format_args!("INFO: [{}:{}:{}][time] ", file, line, column).to_string();

        let Some(logger) = embedded_tracker_esp32_s3::log::LOGGER.try_get() else {
            esp_println::println!("Logger not initialized");
            return;
        };

        esp_println::println!("{}{}", prefix, message);
        
        let mut storage_service = logger.storage_service.lock().await;
        storage_service.append_to_session_log(prefix.as_bytes());
        storage_service.append_to_session_log(message.as_bytes());
        storage_service.append_to_session_log("\n".as_bytes());
    }}
}