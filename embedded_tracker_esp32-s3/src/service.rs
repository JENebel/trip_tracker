use core::ops::Deref;
use alloc::fmt::Debug;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

extern crate alloc;
use alloc::sync::Arc;
use alloc::boxed::Box;

// Define the clean Service trait
#[async_trait::async_trait]
pub trait Service: Debug {
    async fn stop(&mut self);
}

#[derive(Debug)]
pub struct ExclusiveService<S: Service> {
    service: Arc<Mutex<CriticalSectionRawMutex, S>>,
}

impl<S: Service + Debug> ExclusiveService<S> {
    pub fn new(service: Arc<Mutex<CriticalSectionRawMutex, S>>) -> Self {
        Self {
            service,
        }
    }
}

impl<S: Service> Clone for ExclusiveService<S> {
    fn clone(&self) -> Self {
        Self {
              service: self.service.clone(),
        }
    }
}

impl<S: Service> Deref for ExclusiveService<S> {
    type Target = Mutex<CriticalSectionRawMutex, S>;
    
    fn deref(&self) -> &Self::Target {
        &self.service
    }
}