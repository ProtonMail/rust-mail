#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
use chrono::DateTime;
use futures::TryFutureExt;
use muon::common::{BoxFut, Sender, SenderLayer};
use muon::error::ParseAppVersionErr;
use muon::{App, ProtonRequest, ProtonResponse};
use proton_crypto_account::proton_crypto::crypto::UnixTimestamp;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::auth::{Auth, UserKeySecret};
use crate::crypto_clock::{init_server_crypto_clock, server_crypto_clock};
use crate::service::ApiServiceResult;
use crate::services::proton::Proton;
use crate::store::{DynStore, Store, TempStore};

pub use muon::app::AppVersion;
pub use muon::common::{Endpoint, Server};
pub use muon::env::{Env, EnvId};
pub use muon::tls::TlsPinSet;

/// Core session trait which provides access to the API.
pub trait CoreSession {
    #[must_use]
    fn api(&self) -> &Proton;
}

impl CoreSession for Session {
    fn api(&self) -> &Proton {
        &self.client
    }
}

/// An error that can occur when creating or using a session.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum SessionError {
    Muon(#[from] muon::Error),
    AppVersion(#[from] ParseAppVersionErr),
}

/// A session configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// The app version to report (`x-pm-appversion`).
    pub app_version: String,

    /// The user agent to report, if any.
    pub user_agent: Option<String>,

    /// The environment to connect to.
    pub env_id: EnvId,
}

impl Config {
    pub fn atlas() -> Self {
        Self {
            app_version: String::from("Other"),
            user_agent: None,
            env_id: EnvId::new_atlas(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app_version: String::from("Other"),
            user_agent: None,
            env_id: EnvId::new_prod(),
        }
    }
}

/// An API session, capable of making requests to the API on behalf of a user.
#[derive(Clone)]
pub struct Session {
    client: Proton,
    store: DynStore,
}

impl Session {
    /// Create a new Session
    ///
    /// # Errors
    ///
    /// Returns error if the API service failed to initialize.
    pub async fn new(cfg: Config, store: Option<Box<dyn Store>>) -> Result<Self, SessionError> {
        init_server_crypto_clock();

        let app = if let Some(agent) = cfg.user_agent {
            App::new(&cfg.app_version)?.with_user_agent(agent)
        } else {
            App::new(&cfg.app_version)?
        };

        let store = if let Some(store) = store {
            Arc::new(RwLock::new(store))
        } else {
            Arc::new(RwLock::new(TempStore::boxed()))
        };

        let wrapped = MuonStore(cfg.env_id, Arc::clone(&store));
        let builder = Proton::builder(app, wrapped);
        let client = builder.layer_front(SetCryptoClockLayer).build()?;

        Ok(Self { client, store })
    }

    /// Fork the current session.
    ///
    /// This call has to be made from a parent session, and forks the current
    /// logged-in user session in order to provide a new session for the same
    /// user.
    ///
    /// If successful, this will return the "Selector" string for the new
    /// session.
    ///
    /// # Errors
    ///
    /// Any of the [`ApiServiceError`] variants could be returned if there is a
    /// problem with the HTTP request.
    ///
    pub async fn fork(&self) -> ApiServiceResult<String> {
        todo!()
    }

    /// Fork the current session with a user and a version.
    /// for more details see [`Fork`]
    ///
    /// # Errors
    ///
    /// Any of the [`ApiServiceError`] variants could be returned if there is a
    /// problem with the HTTP request.
    ///
    pub async fn fork_with_version(&self, _: String) -> ApiServiceResult<String> {
        todo!()
    }

    /// Exposes the user key secret from the auth store to unlock user keys.
    ///
    /// Returns [`None`] if the auth store is not available or no key secret is
    /// stored.
    ///
    pub async fn expose_key_secret(&self) -> Option<UserKeySecret> {
        self.store.read().await.get_key_secret().await
    }

    /// Logout the user and invalidate the current session.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    pub async fn logout(&self) -> ApiServiceResult<()> {
        self.client.logout().await;
        self.store.write().await.clear().await?;

        Ok(())
    }
}

impl Session {
    pub(crate) fn into_parts(self) -> (Proton, DynStore) {
        (self.client, self.store)
    }

    pub(crate) fn from_parts(client: Proton, store: DynStore) -> Self {
        Self { client, store }
    }
}

/// Implements the muon store trait for our store type.
struct MuonStore<S>(EnvId, Arc<RwLock<S>>);

#[async_trait]
impl<S: Store + 'static> muon::store::Store for MuonStore<S> {
    fn env(&self) -> EnvId {
        self.0.clone()
    }

    async fn get_auth(&self) -> Auth {
        self.1.read().await.get_auth().await
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<Auth, muon::store::StoreError> {
        let mut store = self.1.write().await;

        store
            .set_auth(auth)
            .map_err(|_| muon::store::StoreError)
            .await?;

        Ok(store.get_auth().await)
    }
}

struct SetCryptoClockLayer;

impl SetCryptoClockLayer {
    async fn on_send(
        &self,
        inner: &dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> muon::Result<ProtonResponse> {
        let res = inner.send(req).await?;

        if let Some(date) = res
            .headers()
            .get("date")
            .and_then(|response_time_header| response_time_header.to_str().ok())
            .and_then(|response_time| DateTime::parse_from_rfc2822(response_time).ok())
            .and_then(|parsed_server_time| parsed_server_time.timestamp().try_into().ok())
            .map(UnixTimestamp)
        {
            server_crypto_clock().update_clock(date);
        }

        Ok(res)
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetCryptoClockLayer {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, muon::Result<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}
