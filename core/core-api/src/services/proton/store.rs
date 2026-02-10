use async_trait::async_trait;
use futures::TryFutureExt;
use muon::client::Auth;
use muon::env::EnvId;
use muon::store::{Store as MuonStore, StoreError as MuonStoreError};
use std::sync::Weak;
use std::{borrow::Borrow, sync::Arc};
use tokio::sync::RwLock;

use crate::store::Store;

/// Implements the muon store trait for our store type.
pub struct MuonStoreImpl<S> {
    env_id: EnvId,
    store: Weak<RwLock<S>>,
}

impl<S> MuonStoreImpl<S> {
    pub fn new(env_id: impl Borrow<EnvId>, store: impl Borrow<Arc<RwLock<S>>>) -> Self {
        let env_id = env_id.borrow().to_owned();
        let store = Arc::downgrade(store.borrow());

        Self { env_id, store }
    }
}

#[async_trait]
impl<S: Store> MuonStore for MuonStoreImpl<S> {
    fn env(&self) -> EnvId {
        self.env_id.clone()
    }

    async fn get_auth(&self) -> Auth {
        if let Some(store) = self.store.upgrade() {
            store.read().await.get_auth().await
        } else {
            Auth::None
        }
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<Auth, MuonStoreError> {
        if let Some(store) = self.store.upgrade() {
            store
                .write()
                .await
                .set_auth(auth)
                .map_err(|_| MuonStoreError)
                .await?;

            Ok(self.get_auth().await)
        } else {
            Err(MuonStoreError)
        }
    }
}
