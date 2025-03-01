use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use heapless::Vec;

use alloc::sync::Arc;

use crate::{info, ExclusiveService, Service};

pub struct SystemControl {
    services: Vec<Arc<Mutex<CriticalSectionRawMutex, dyn Service>>, 12>,
}

impl SystemControl {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    pub async fn register_and_start_service<S: Service + Sized + 'static>(&mut self, mut service: S) -> ExclusiveService<S> {
        service.start().await;
        let service = Arc::new(Mutex::<CriticalSectionRawMutex, S>::new(service));
        match self.services.push(service.clone()) {
            Ok(_) => {}
            Err(_) => panic!("Failed to register service"),
        }
        ExclusiveService::new(service)
    }

    pub async fn start_services(&mut self) {
        for service in self.services.iter() {
            info!("Starting {:?}", service);
            service.lock().await.start().await;
        }
    }

    // Stop in reversed order, to ensure dependencies are stopped first
    pub async fn stop_services(&mut self) {
        for service in self.services.iter().rev() {
            info!("Stopping {:?}", service);
            service.lock().await.stop().await;
        }
    }
}