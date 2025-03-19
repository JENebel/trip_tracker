use alloc::sync::Arc;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{Duration, TimeoutError, WithTimeout};

use crate::ByteBuffer;

const SIZE: usize = 256;

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

    /**
     * Will return when out_buffer is filled with data or the timeout is reached.
     */
    pub async fn read_exact_timeout(&self, out_buffer: &mut [u8], timeout: u64) -> Result<(), TimeoutError> {
        self.read_exact_block(out_buffer).with_timeout(Duration::from_millis(timeout)).await
    }

    /**
     * Will never return until the buffer has enough data to fill the out_buffer.
     */
    pub async fn read_exact_block(&self, out_buffer: &mut [u8]) {
        loop {
            let available = self.notifier.wait().await;
            self.notifier.reset();
            if available >= out_buffer.len() {
                let mut buffer = self.buffer.lock().await;
                let content = buffer.pop(out_buffer.len());
                out_buffer.copy_from_slice(content);

                buffer.shift_back();
                return;
            }
        }
    }
}