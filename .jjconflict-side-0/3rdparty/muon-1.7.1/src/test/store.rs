use crate::env::{Env, EnvId};
use crate::store::{Store, StoreError};
use crate::Auth;
use async_trait::async_trait;
use std::sync::{Arc, RwLock};

/// A simple in-memory store.
#[must_use]
#[derive(Debug, Clone)]
pub struct TestStore(EnvId, Arc<RwLock<Auth>>);

impl Default for TestStore {
    fn default() -> Self {
        Self::prod()
    }
}

impl TestStore {
    /// Create a new test store for the given environment.
    pub fn new(env: EnvId) -> Self {
        Self(env, Arc::default())
    }

    /// Create a new prod store.
    pub fn prod() -> Self {
        Self::new(EnvId::new_prod())
    }

    /// Create a new atlas store.
    pub fn atlas() -> Self {
        Self::new(EnvId::new_atlas())
    }

    /// Create a new atlas store with the given name.
    pub fn atlas_name(name: impl AsRef<str>) -> Self {
        Self::new(EnvId::new_atlas_name(name))
    }

    /// Create a new custom store.
    pub fn custom(env: impl Env) -> Self {
        Self::new(EnvId::new_custom(env))
    }
}

#[async_trait]
impl Store for TestStore {
    fn env(&self) -> EnvId {
        self.0.clone()
    }

    async fn get_auth(&self) -> Auth {
        let lock = self.1.read().expect("lock should succeed");
        lock.clone()
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<Auth, StoreError> {
        let mut lock = self.1.write().expect("lock should succeed");
        *lock = auth.clone();
        Ok(auth)
    }
}
