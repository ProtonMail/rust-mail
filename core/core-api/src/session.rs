#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
use chrono::DateTime;
use futures::TryFutureExt;
use muon::client::flow::ForkFlowResult;
use muon::common::{BoxFut, IntoDyn, Sender, SenderLayer, ServiceType};
use muon::dns::{GoogleDoh, Quad9Doh};
use muon::Result as MuonResult;
use muon::{App, ProtonRequest, ProtonResponse};
use proton_crypto_account::proton_crypto::crypto::UnixTimestamp;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::auth::{Auth, UserKeySecret};
use crate::crypto_clock::{init_server_crypto_clock, server_crypto_clock};
use crate::service::ApiServiceResult;
use crate::services::proton::Proton;
use crate::store::{DynStore, Store, TempStore};

pub use muon::app::AppVersion;
pub use muon::common::{Endpoint, Server};
pub use muon::env::{Env, EnvId};
pub use muon::error::ParseAppVersionErr;
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
    #[must_use]
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
    config: Config,
    store: DynStore,
}

impl Session {
    /// Create a new Session
    ///
    /// # Errors
    ///
    /// Returns error if the API service failed to initialize.
    ///
    /// # Panics
    ///
    /// Panics if the Proton client fails to build.
    pub fn new(config: Config, store: Option<Box<dyn Store>>) -> Result<Self, ParseAppVersionErr> {
        init_server_crypto_clock();

        let app = if let Some(agent) = &config.user_agent {
            App::new(&config.app_version)?.with_user_agent(agent)
        } else {
            App::new(&config.app_version)?
        };

        let store = if let Some(store) = store {
            Arc::new(RwLock::new(store))
        } else {
            Arc::new(RwLock::new(TempStore::boxed()))
        };

        let client = Proton::builder(app, MuonStore::new(&config.env_id, &store))
            .doh([Quad9Doh.into_dyn(), GoogleDoh.into_dyn()])
            .layer_back(SetCryptoClockLayer)
            .layer_back(SetDefaultServiceTypeLayer)
            .layer_back(SetDefaultTimeoutLayer)
            .build()
            .expect("Proton client must be built successfully");

        Ok(Self {
            client,
            config,
            store,
        })
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
        self.fork_with_version(&self.config.app_version).await
    }

    /// Fork the current session with a user and a version.
    /// for more details see [`Fork`]
    ///
    /// # Errors
    ///
    /// Any of the [`ApiServiceError`] variants could be returned if there is a
    /// problem with the HTTP request.
    ///
    pub async fn fork_with_version(&self, version: impl AsRef<str>) -> ApiServiceResult<String> {
        match self.client.clone().fork(version.as_ref()).send().await {
            ForkFlowResult::Success(_, selector) => Ok(selector),
            ForkFlowResult::Failure { reason, .. } => Err(muon::Error::from(reason))?,
        }
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

/// The parts of a session.
pub(crate) struct SessionParts {
    pub(crate) client: Proton,
    pub(crate) config: Config,
    pub(crate) store: DynStore,
}

impl Session {
    pub(crate) fn into_parts(self) -> SessionParts {
        SessionParts {
            client: self.client,
            config: self.config,
            store: self.store,
        }
    }

    pub(crate) fn from_parts(parts: SessionParts) -> Self {
        Self {
            client: parts.client,
            config: parts.config,
            store: parts.store,
        }
    }
}

/// Implements the muon store trait for our store type.
struct MuonStore<S>(EnvId, Arc<RwLock<S>>);

impl<S> MuonStore<S> {
    fn new(env_id: &EnvId, store: &Arc<RwLock<S>>) -> Self {
        Self(env_id.to_owned(), Arc::clone(store))
    }
}

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
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let response = inner.send(req).await?;

        if let Some(date) = response
            .headers()
            .get("date")
            .and_then(|response_time_header| response_time_header.to_str().ok())
            .and_then(|response_time| DateTime::parse_from_rfc2822(response_time).ok())
            .and_then(|parsed_server_time| parsed_server_time.timestamp().try_into().ok())
            .map(UnixTimestamp)
        {
            server_crypto_clock().update_clock(date);
        }

        Ok(response)
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetCryptoClockLayer {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

struct SetDefaultServiceTypeLayer;

impl SetDefaultServiceTypeLayer {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let req = if req.get_service_type().is_none() {
            req.service_type(ServiceType::default(), true)
        } else {
            req
        };

        inner.send(req).await
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetDefaultServiceTypeLayer {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

struct SetDefaultTimeoutLayer;

impl SetDefaultTimeoutLayer {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

        // NOTE: This is not a bug! Muon logs a warning if no timeout is explicitly set;
        // this workaround sets the timeout explicitly if it was not already set to a
        // non-default value earlier in the layer stack.
        let req = if req.get_allowed_time() == &DEFAULT_TIMEOUT {
            req.allowed_time(DEFAULT_TIMEOUT)
        } else {
            req
        };

        inner.send(req).await
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetDefaultTimeoutLayer {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}
