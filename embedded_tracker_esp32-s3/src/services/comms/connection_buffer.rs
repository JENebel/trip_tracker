use alloc::sync::Arc;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex, signal::Signal};
use esp_println::println;

use crate::ByteBuffer;

const SIZE: usize = 1024;

#[derive(Clone)]
pub struct ConnectionBuffer {
    buffer: Arc<Mutex<CriticalSectionRawMutex, ByteBuffer<SIZE>>>,
    notifier: Arc<Signal<CriticalSectionRawMutex, usize>>,
}

impl ConnectionBuffer {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(ByteBuffer::new())),
            notifier: Arc::new(Signal::new()),
        }
    }

    pub async fn write(&self, data: &[u8]) {
        let mut buffer = self.buffer.lock().await;
        buffer.push(data);
        self.notifier.signal(buffer.len());
    }

    pub async fn read_exact(&self, out_buffer: &mut [u8]) {
        loop {
            let available = self.notifier.wait().await;
            self.notifier.reset();
            if available >= out_buffer.len() {
                let mut buffer = self.buffer.lock().await;
                let content = buffer.pop(out_buffer.len());
                out_buffer.copy_from_slice(content);
                return;
            }
        }
    }
}