use core::primitive;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_println::println;
use heapless::Vec;

use alloc::sync::Arc;

use crate::{ExclusiveService, Service};

pub struct SystemControl {
    services: Vec<Arc<Mutex<CriticalSectionRawMutex, dyn Service>>, 12>,
}

impl SystemControl {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    pub fn register_service<S: Service + Sized + 'static>(&mut self, service: S) -> ExclusiveService<S> {
        let service = Arc::new(Mutex::<CriticalSectionRawMutex, S>::new(service));
        match self.services.push(service.clone()) {
            Ok(_) => {}
            Err(_) => panic!("Failed to register service"),
        }
        ExclusiveService::new(service)
    }

    pub async fn start_services(&mut self) {
        for service in self.services.iter() {
            println!("Starting {:?}", service);
            service.lock().await.start().await;
        }
    }

    // Stop in reversed order, to ensure dependencies are stopped first
    pub async fn stop_services(&mut self) {
        for service in self.services.iter().rev() {
            println!("Stopping {:?}", service);
            service.lock().await.stop().await;
        }
    }
}