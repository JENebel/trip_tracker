use core::{future::Future, sync::atomic::{AtomicBool, Ordering}};

use alloc::sync::Arc;
use embassy_futures::select::Either;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

#[derive(Clone)]
pub struct ActorControl {
    toggle_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,

    stopped_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,

    is_running: Arc<AtomicBool>,
}

impl ActorControl {
    pub fn new() -> Self {
        let toggle_signal = Arc::new(Signal::new());

        let stopped_signal = Arc::new(Signal::new());

        let is_running = Arc::new(AtomicBool::new(false));

        Self {
            toggle_signal,
            stopped_signal,
            is_running,
        }
    }

    pub async fn wait_for_start(&self) {
        if self.is_running.load(Ordering::Relaxed) {
            return;
        }
        loop {
            self.toggle_signal.wait().await;
            if self.is_running.load(Ordering::Relaxed) {
                return;
            }
        }
    }

    pub async fn start(&self) {
        self.is_running.store(true, Ordering::Relaxed);
        self.toggle_signal.signal(true);
    }

    pub async fn stop(&self) {
        self.stopped_signal.reset();
        self.is_running.store(false, Ordering::Relaxed);
        self.toggle_signal.signal(false);

        self.stopped_signal.wait().await;
        self.stopped_signal.reset();
    }

    pub fn stopped(&self) {
        self.stopped_signal.signal(true);
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }
}