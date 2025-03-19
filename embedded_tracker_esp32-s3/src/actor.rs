use core::{future::Future, sync::atomic::{AtomicBool, Ordering}};

use alloc::sync::Arc;
use embassy_futures::select::Either;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

#[derive(Clone)]
pub struct ActorControl {
    start_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,
    stop_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,

    started_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,
    stopped_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,

    is_stopped: Arc<AtomicBool>,
}

impl ActorControl {
    pub fn new() -> Self {
        let start_signal = Arc::new(Signal::new());
        let stop_signal = Arc::new(Signal::new());

        let started_signal = Arc::new(Signal::new());
        let stopped_signal = Arc::new(Signal::new());

        let is_running = Arc::new(AtomicBool::new(false));

        Self {
            start_signal,
            stop_signal,
            started_signal,
            stopped_signal,
            is_stopped: is_running,
        }
    }

    /// Returns error immediately if cancelled
    pub async fn run_cancelable<Fut, R>(&mut self, future: Fut) -> Result<R, ()>
    where
        Fut: Future<Output = R>,
    {
        match embassy_futures::select::select(
                self.wait_for_stop(),
                future
            ).await {
                Either::First(_) => Err(()),
                Either::Second(r) => Ok(r),
            }
    }

    pub async fn wait_for_start(&self) {
        if !self.is_stopped.load(Ordering::Relaxed) {
            return;
        }
        self.start_signal.wait().await;
        self.start_signal.reset();
        self.is_stopped.store(false, Ordering::Relaxed);
    }
    
    async fn wait_for_stop(&self) {
        if self.is_stopped.load(Ordering::Relaxed) {
            return;
        }
        self.stop_signal.wait().await;
        self.stop_signal.reset();
    }

    pub async fn start(&self) {
        self.is_stopped.store(false, Ordering::Relaxed);
        self.start_signal.signal(true);
        self.started_signal.wait().await;
        self.started_signal.reset();
    }

    pub async fn stop(&self) {
        self.is_stopped.store(true, Ordering::Relaxed);
        self.stop_signal.signal(true);
        self.stopped_signal.wait().await;
        self.stopped_signal.reset();
    }

    pub fn stopped(&self) {
        self.stopped_signal.signal(true);
    }

    pub fn started(&self) {
        self.started_signal.signal(true);
    }

    pub fn is_stopped(&self) -> bool {
        self.is_stopped.load(Ordering::Relaxed)
    }
}