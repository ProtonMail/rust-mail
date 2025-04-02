use std::{collections::VecDeque, ops::Deref, sync::Arc};

use tokio::sync::RwLock;

use crate::connection_status::ConnectionStatus;

#[derive(Clone, Debug)]
pub struct FixedQueue<T> {
    queue: VecDeque<T>,
    capacity: usize,
}

impl<T> FixedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.queue.len() == self.capacity {
            self.queue.pop_front(); // Remove oldest element
        }
        self.queue.push_back(item);
    }
}

impl<T> Deref for FixedQueue<T> {
    type Target = VecDeque<T>;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

#[derive(Clone, Debug)]
pub struct StatusChanges {
    queue: Arc<RwLock<FixedQueue<ConnectionStatus>>>,
}

impl StatusChanges {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Arc::new(RwLock::new(FixedQueue::new(capacity))),
        }
    }

    pub async fn push(&self, status: ConnectionStatus) {
        self.queue.write().await.push(status);
    }

    pub async fn was_online_most_of_the_time(&self) -> bool {
        let queue = self.queue.read().await;

        let online_count = queue.iter().filter(|st| st.is_online()).count();
        let offline_count = queue.iter().filter(|st| st.is_offline()).count();

        online_count > offline_count
    }
}
