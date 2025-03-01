use core::future::Future;

use alloc::sync::Arc;
use embassy_futures::select::Either;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use esp_println::println;

#[derive(Clone)]
pub struct ActorControl {
    start_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,
    stop_signal: Arc<Signal<CriticalSectionRawMutex, bool>>,
    is_running: Arc<Mutex<CriticalSectionRawMutex, bool>>,
}

impl ActorControl {
    pub fn new() -> Self {
        let start_signal = Arc::new(Signal::new());
        let stop_signal = Arc::new(Signal::new());
        let is_running_signal = Arc::new(Mutex::new(false));
        Self {
            start_signal,
            stop_signal,
            is_running: is_running_signal,
        }
    }

    /// Returns error immediately if cancelled
    pub async fn run_cancelable<Fut, R>(&mut self, future: Fut) -> Result<R, ()>
    where
        Fut: Future<Output = R>,
    {
        match embassy_futures::select::select(
                self.wait_for_cancel(),
                future
            ).await {
                Either::First(_) => Err(()),
                Either::Second(r) => Ok(r),
            }
    }

    pub async fn wait_for_start(&self) {
        if *self.is_running.lock().await {
            return;
        }
        self.start_signal.wait().await;
    }
    
    async fn wait_for_cancel(&self) {
        if !*self.is_running.lock().await {
            return;
        }
        self.stop_signal.wait().await;
    }

    pub async fn start(&self) {
        self.start_signal.signal(true);
        *self.is_running.lock().await = true;
    }

    pub async fn stop(&self) {
        *self.is_running.lock().await = false;
        self.stop_signal.signal(true);
    }
}