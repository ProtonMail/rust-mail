use std::{collections::VecDeque, num::NonZeroUsize, ops::Deref, sync::Arc};

use parking_lot::RwLock;

use crate::connection_status::ConnectionStatus;

#[derive(Clone, Debug)]
struct FixedQueue<T> {
    queue: VecDeque<T>,
    capacity: usize,
}

impl<T> FixedQueue<T> {
    fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn push(&mut self, item: T) {
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
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            queue: Arc::new(RwLock::new(FixedQueue::new(capacity.get()))),
        }
    }

    pub fn push(&self, status: ConnectionStatus) {
        self.queue.write().push(status);
    }

    pub fn was_online_most_of_the_time(&self) -> bool {
        let queue = self.queue.read();

        let online_count = queue.iter().filter(|st| st.is_online()).count();
        let offline_count = queue.iter().filter(|st| st.is_offline()).count();

        online_count > offline_count
    }
}
