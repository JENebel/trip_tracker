use core::fmt::Debug;

use alloc::{string::String, sync::Arc};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, once_lock::OnceLock};
use esp_println::{print, println};

use crate::ExclusiveService;

use super::StorageService;

pub static GLOBAL_LOGGER: OnceLock<Logger> = OnceLock::new();

pub struct LogMessage {
    pub message: String,
    pub sys_log: bool,
}

#[derive(Clone)]
pub struct Logger {
    pub log_queue: Arc<Channel<CriticalSectionRawMutex, LogMessage, 10>>,
}

impl Debug for Logger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Gobal logger")
    }
}

impl Logger {
    pub fn start(spawner: &Spawner, storage_service: ExclusiveService<StorageService>) {
        let log_queue = Arc::new(Channel::new());

        if let Some(_logger) = GLOBAL_LOGGER.try_get() {
            crate::error!("Logger already initialized");
        } else {
            GLOBAL_LOGGER.init(Logger {
                log_queue: log_queue.clone(),
            }).unwrap();
            spawner.must_spawn(log_task(storage_service, log_queue));
            crate::debug!("Logger initialized");
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum LogLevel {
    Info,
    Debug,
    Warn,
    Error,
}

#[macro_export]
macro_rules! inner_log {
    ($log_level:expr, $sys_log:expr, $($arg:tt)*) => {'block: {
        extern crate alloc;
        use alloc::string::ToString;

        let message = format_args!($($arg)+).to_string();

        let sys_log: bool = $sys_log;
        let log_level: $crate::log::LogLevel = $log_level;

        let location = if log_level == $crate::log::LogLevel::Error {
            let file = file!();
            let line = line!();
            let column = column!();
            format_args!("[{}:{}:{}]", file, line, column).to_string()
        } else {
            "".to_string()
        };

        let time = esp_hal::time::now().ticks() / 1_000_000u64;
        
        let log = format_args!("{:?}:\t{}[T+{}] {}\n", log_level, location, time, message).to_string();

        let Some(logger) = $crate::log::GLOBAL_LOGGER.try_get() else {
            esp_println::print!("UNINIT {}", log);
            break 'block;
        };

        esp_println::print!("{}", log);

        let message = $crate::log::LogMessage {
            message: log,
            sys_log,
        };
        
        match logger.log_queue.try_send(message) {
            Ok(_) => {},
            Err(_) => {
                logger.log_queue.clear();
                let _ = logger.log_queue.try_send($crate::log::LogMessage {
                    message: "Log queue was cleared because it was full".to_string(),
                    sys_log: true,
                });
            }
        }

        /*let mut storage_service = logger.storage_service.lock();
        if sys_log {
            storage_service.append_to_sys_log(log.as_bytes());
        }
        storage_service.append_to_session_log(message.as_bytes());*/
        }
    }
}

#[embassy_executor::task]
async fn log_task(
    storage_service: ExclusiveService<StorageService>, 
    log_queue: Arc<Channel<CriticalSectionRawMutex, LogMessage, 10>>
) {
    loop {
        let message = log_queue.receive().await;

        let log = message.message.as_bytes();

        let mut storage_service = storage_service.lock().await;
        if message.sys_log {
            storage_service.append_to_sys_log(log);
        }
        if storage_service.append_to_session_log(log).is_err() {
            print!("Failed: {}", message.message);
        }
    }
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        $crate::inner_log!($crate::log::LogLevel::Info, false, $($arg)*);
    }}
}

#[macro_export]
macro_rules! sys_info {
    ($($arg:tt)*) => {{
        $crate::inner_log!($crate::log::LogLevel::Info, true, $($arg)*);
    }}
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        $crate::inner_log!($crate::log::LogLevel::Debug, false, $($arg)*);
    }}
}

#[macro_export]
macro_rules! sys_debug {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        $crate::inner_log!($crate::log::LogLevel::Debug, true, $($arg)*);
    }}
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        crate::inner_log!(crate::log::LogLevel::Warn, false, $($arg)*);
    }}
}

#[macro_export]
macro_rules! sys_warn {
    ($($arg:tt)*) => {{
        $crate::inner_log!($crate::log::LogLevel::Warn, true, $($arg)*);
    }}
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        $crate::inner_log!($crate::log::LogLevel::Error, true, $($arg)*);
    }}
}