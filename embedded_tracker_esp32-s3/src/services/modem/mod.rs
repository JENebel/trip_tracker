mod modem;
mod urc_subscriber_set;

pub use modem::ModemService;
pub use urc_subscriber_set::URCSubscriber;

pub const MAX_RESPONSE_LENGTH: usize = 256;