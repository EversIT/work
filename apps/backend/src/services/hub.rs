//! Per-user realtime fan-out for server-sent events. A user only ever receives messages
//! addressed to their own user id; there is no client-chosen room name.

use std::{collections::HashMap, sync::Mutex};

use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Default)]
pub struct Hub {
    channels: Mutex<HashMap<Uuid, broadcast::Sender<String>>>,
}

impl Hub {
    pub fn subscribe(&self, user: Uuid) -> broadcast::Receiver<String> {
        let mut map = self.channels.lock().unwrap_or_else(|p| p.into_inner());
        map.entry(user)
            .or_insert_with(|| broadcast::channel(64).0)
            .subscribe()
    }

    pub fn publish(&self, user: Uuid, message: String) {
        let mut map = self.channels.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(tx) = map.get(&user) {
            if tx.send(message).is_err() {
                map.remove(&user); // nobody is listening any more
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delivers_only_to_the_addressed_user() {
        let hub = Hub::default();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut ra = hub.subscribe(a);
        let mut rb = hub.subscribe(b);
        hub.publish(a, "for a".into());
        assert_eq!(ra.recv().await.unwrap(), "for a");
        assert!(rb.try_recv().is_err());
    }
}
