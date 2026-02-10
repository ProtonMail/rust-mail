use crate::common::Endpoint;
use crate::http::DynHttpSender;
use derive_more::Deref;
use futures::lock::{Mutex, MutexGuard};
use std::collections::HashMap;
use std::sync::{Arc, Weak};

/// An attempt to get a sender from the pool.
///
/// If it succeeds, it returns the value.
/// Otherwise, it returns the pool guard, enabling a retry.
#[derive(Debug)]
pub enum TryGet<'a, T> {
    Some(T),
    None(PoolGuard<'a>),
}

/// A pool of HTTP senders.
///
/// This is protected by a mutex.
/// All pool operations must begin by acquiring a lock.
#[derive(Debug, Default)]
pub struct Pool {
    items: Mutex<HashMap<Endpoint, DynHttpSender>>,
}

impl Pool {
    /// Acquire a lock on the pool.
    ///
    /// This returns a [`PoolGuard`] that releases the lock when dropped.
    pub async fn lock<'a>(self: &'a Arc<Self>) -> PoolGuard<'a> {
        PoolGuard {
            pool: self,
            lock: self.items.lock().await,
        }
    }
}

/// A guard enabling the pool to be modified.
#[derive(Debug)]
pub struct PoolGuard<'a> {
    pool: &'a Arc<Pool>,
    lock: MutexGuard<'a, HashMap<Endpoint, DynHttpSender>>,
}

impl<'a> PoolGuard<'a> {
    /// Get a sender from the pool.
    pub fn get(self, key: &Endpoint) -> TryGet<'a, PooledSender> {
        if let Some(sender) = self.lock.get(key) {
            TryGet::Some(PooledSender::new(key, self.pool, sender))
        } else {
            TryGet::None(self)
        }
    }

    /// Set the sender for the given target URL.
    pub fn insert(mut self, key: &Endpoint, sender: DynHttpSender) -> PooledSender {
        self.lock.insert(key.clone(), sender.clone());

        PooledSender::new(key, self.pool, &sender)
    }

    /// Extend the sender's lifetime in the pool.
    ///
    /// TODO: Do we need this?
    fn extend(&mut self, _: &Endpoint) {
        let _ = self;
    }

    /// Remove the sender with the given key.
    fn unpool(&mut self, key: &Endpoint) {
        self.lock.remove(key);
    }
}

/// A pool sender.
///
/// This type wraps a sender and a weak reference to the pool.
/// If a send fails, we assume the sender is broken and remove it from the pool;
/// this is achieved by calling the sender's `unpool` method.
#[derive(Debug, Deref)]
pub struct PooledSender {
    /// The key by which this sender can be found in the pool.
    key: Endpoint,

    /// The pool from which this sender was taken.
    pool: Weak<Pool>,

    /// The sender itself.
    #[deref]
    sender: DynHttpSender,
}

impl PooledSender {
    fn new(key: &Endpoint, pool: &Arc<Pool>, sender: &DynHttpSender) -> Self {
        let key = key.to_owned();
        let pool = Arc::downgrade(pool);
        let sender = sender.to_owned();

        Self { key, pool, sender }
    }

    /// Extend the sender's lifetime in the pool.
    pub async fn repool(self) {
        if let Some(pool) = self.pool.upgrade() {
            trace!(server = %self.key, "repooling sender");
            pool.lock().await.extend(&self.key);
        }
    }

    /// Remove the sender from the pool.
    pub async fn unpool(self) {
        if let Some(pool) = self.pool.upgrade() {
            warn!(server = %self.key, "unpooling sender");
            pool.lock().await.unpool(&self.key);
        }
    }
}
