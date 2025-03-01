use core::{fmt::{self, Debug}, sync::atomic::{AtomicU8, Ordering}};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex};
use heapless::Vec;

extern crate alloc;
use alloc::{string::String, sync::Arc};

use crate::warn;

pub const URC_CHANNEL_SIZE: usize = 10;

#[derive(Clone)]
pub struct URCSubscriber<const N: usize> {
    pub id: u8,
    pub urc: &'static str,
    pub channel: Arc<Channel<CriticalSectionRawMutex, String, N>>
}

impl<const N: usize> Debug for URCSubscriber<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "URCSubscriber {{ id: {}, urc: {} }}", self.id, self.urc)
    }
}

impl URCSubscriber<1> {
    fn new_one_shot(urc: &'static str, id: u8) -> Self {
        let channel = Arc::new(Channel::new());
        Self {
            id,
            urc,
            channel,
        }
    }
}

impl URCSubscriber<URC_CHANNEL_SIZE> {
    fn new(urc: &'static str, id: u8) -> Self {
        let channel = Arc::new(Channel::new());
        Self {
            id,
            urc,
            channel,
        }
    }
}

impl<const N: usize> URCSubscriber<N> {
    pub async fn send(&self, response: String) {
        if self.channel.try_send(response).is_err() {
            warn!("URCSubscriber channel full, dropping response");
        }
    }

    pub async fn receive(&self) -> String {
        self.channel.receive().await
    }
}

#[derive(Default, Clone)]
pub struct URCSubscriberSet<const N: usize> {
    urc_subscribers: Arc<Mutex<CriticalSectionRawMutex, Vec<URCSubscriber<URC_CHANNEL_SIZE>, N>>>,
    next_urc_subscriber_id: Arc<AtomicU8>,
    urc_oneshot_subscribers: Arc<Mutex<CriticalSectionRawMutex, Vec<URCSubscriber<1>, N>>>,
    next_urc_oneshot_id: Arc<AtomicU8>,
}

impl<const N: usize> URCSubscriberSet<N> {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn add(&mut self, urc: &'static str) -> URCSubscriber<URC_CHANNEL_SIZE> {
        let mut guard = self.urc_subscribers.lock().await;
        let id = self.next_urc_subscriber_id.fetch_add(1, Ordering::Relaxed);
        let subscriber = URCSubscriber::new(urc, id);
        guard.push(subscriber.clone()).unwrap();
        subscriber
    }

    pub async fn add_oneshot(&mut self, urc: &'static str) -> URCSubscriber<1> {
        let mut guard = self.urc_oneshot_subscribers.lock().await;
        let id = self.next_urc_oneshot_id.fetch_add(1, Ordering::Relaxed);
        let subscriber = URCSubscriber::new_one_shot(urc, id);
        guard.push(subscriber.clone()).unwrap();
        subscriber
    }

    pub async fn remove(&mut self, id: u8) {
        let mut guard = self.urc_subscribers.lock().await;
        guard.retain(|subscriber| subscriber.id != id);
    }

    pub async fn remove_oneshot(&mut self, id: u8) {
        let mut guard = self.urc_oneshot_subscribers.lock().await;
        guard.retain(|subscriber| subscriber.id != id);
    }

    pub async fn send(&self, urc: &str, response: String) {
        let guard = self.urc_oneshot_subscribers.lock().await;
        for subscriber in guard.iter() {
            if subscriber.urc == urc {
                subscriber.send(response.clone()).await;
            }
        }

        let guard = self.urc_subscribers.lock().await;
        for subscriber in guard.iter() {
            if subscriber.urc == urc {
                subscriber.send(response.clone()).await;
            }
        }
    }
}