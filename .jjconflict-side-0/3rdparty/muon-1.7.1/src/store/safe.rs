use crate::auth::Auth;
use crate::env::EnvId;
use crate::store::{DynStore, Store};
use async_lock::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use derive_more::{Deref, DerefMut, Display};
use std::sync::Arc;

/// A wrapper around a store that ensures safe access.
#[derive(Debug, Clone)]
pub struct SafeStore {
    env: EnvId,
    hdl: Arc<RwLock<StoreHandle>>,
}

impl SafeStore {
    /// Create a new safe auth store.
    pub fn new(store: impl Store) -> Self {
        let env = store.env();
        let hdl = Arc::new(RwLock::new(StoreHandle::new(store)));

        Self { env, hdl }
    }

    /// Get the environment to which this store is bound.
    pub fn env(&self) -> &EnvId {
        &self.env
    }

    /// Lock the store for reading.
    pub async fn read(&self) -> StoreReadGuard<'_> {
        trace!("locking store for reading");
        let rg = self.hdl.read().await;
        trace!("store now locked for reading");

        StoreReadGuard(rg)
    }

    /// A convenience method to get the current auth.
    ///
    /// This is equivalent to calling `read().await.get_auth().await`.
    pub async fn get_auth(&self) -> (AuthVersion, Auth) {
        self.read().await.get_auth().await
    }

    /// Lock the store for writing.
    pub async fn write(&self) -> StoreWriteGuard<'_> {
        trace!("locking store for writing");
        let wg = self.hdl.write().await;
        trace!("store now locked for writing");

        StoreWriteGuard(wg)
    }

    /// A convenience method to set the current auth.
    ///
    /// This is equivalent to calling `write().await.set_auth(auth).await`.
    pub async fn set_auth(&self, auth: Auth) -> AuthVersion {
        self.write().await.set_auth(auth).await
    }
}

/// A read guard for a safe auth store which adds logging.
#[derive(Debug, Deref)]
pub struct StoreReadGuard<'a>(RwLockReadGuard<'a, StoreHandle>);

impl Drop for StoreReadGuard<'_> {
    fn drop(&mut self) {
        trace!("releasing read lock on store");
    }
}

/// A write guard for a safe auth store which adds logging.
#[derive(Debug, Deref, DerefMut)]
pub struct StoreWriteGuard<'a>(RwLockWriteGuard<'a, StoreHandle>);

impl Drop for StoreWriteGuard<'_> {
    fn drop(&mut self) {
        trace!("releasing write lock on store");
    }
}

/// An auth version.
///
/// This is used to track changes to the auth data.
/// Each time the auth data is updated, the version is incremented.
#[derive(Debug, Display, Default, Clone, Copy, PartialEq, Eq)]
pub struct AuthVersion(usize);

impl AuthVersion {
    fn upgrade(&mut self) {
        self.0 += 1;
    }
}

/// A handle to a store.
///
/// This is the type used to actually interact with the foreign store.
/// It tracks changes to the auth data; each write increments the version.
#[derive(Debug)]
pub struct StoreHandle {
    store: DynStore,
    version: AuthVersion,
}

impl StoreHandle {
    fn new(store: impl Store) -> Self {
        Self {
            store: Box::new(store),
            version: AuthVersion::default(),
        }
    }

    /// Get the current auth and its version.
    pub async fn get_auth(&self) -> (AuthVersion, Auth) {
        (self.version, self.store.get_auth().await)
    }

    /// Set the current auth and return the new version.
    pub async fn set_auth(&mut self, auth: Auth) -> AuthVersion {
        if self.store.set_auth(auth).await.is_ok() {
            self.version.upgrade();
        }

        self.version
    }
}
