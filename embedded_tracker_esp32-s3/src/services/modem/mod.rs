pub mod modem_service;
mod urc_subscriber_set;

pub use modem_service::ModemService;
pub use urc_subscriber_set::URCSubscriber;

pub const MAX_RESPONSE_LENGTH: usize = 256;