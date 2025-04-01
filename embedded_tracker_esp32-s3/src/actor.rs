use core::sync::atomic::{AtomicBool, Ordering};

use alloc::sync::Arc;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

#[derive(Clone)]
pub struct ActorTerminator {
    terminated_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,
    is_terminated: Arc<AtomicBool>,
}

impl ActorTerminator {
    pub fn new() -> Self {
        let stopped_signal = Arc::new(Signal::new());
        let is_stopped = Arc::new(AtomicBool::new(false));

        Self {
            terminated_signal: stopped_signal,
            is_terminated: is_stopped,
        }
    }

    pub async fn terminate(&self) {
        self.is_terminated.store(true, Ordering::Relaxed);
        self.terminated_signal.wait().await;
    }

    pub fn terminated(&self) {
        self.terminated_signal.signal(true);
    }

    pub fn is_terminating(&self) -> bool {
        self.is_terminated.load(Ordering::Relaxed)
    }
}