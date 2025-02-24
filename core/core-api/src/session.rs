#![allow(clippy::module_name_repetitions)]

use derive_more::Debug;
use muon::client::flow::ForkFlowResult;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::UserKeySecret;
use crate::connection_status::ConnectionStatus;
use crate::crypto_clock::init_server_crypto_clock;
use crate::service::ApiServiceResult;
use crate::services::proton::{self, BuildError, Proton};
use crate::status_observer::StatusObserver;
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
#[derive(Debug, Clone)]
#[debug("Session {{ client: {client:?}, config: {config:?} }}")]
pub struct Session {
    client: Proton,
    config: Arc<Config>,
    status: StatusObserver,
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
    pub fn new(
        config: Config,
        store: Option<Box<dyn Store>>,
        status: StatusObserver,
    ) -> Result<Self, BuildError> {
        init_server_crypto_clock();

        let store = Arc::new(RwLock::new(store.unwrap_or_else(|| TempStore::boxed())));
        let client = proton::build(Config::clone(&config), Arc::clone(&store), status.clone())?;
        let config = Arc::new(config);

        Ok(Self {
            client,
            config,
            status,
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
        self.store.read().await.expose_key_secret().await
    }

    /// Logout the user and invalidate the current session.
    ///
    /// # Errors
    ///
    /// This method will return an error if the database query fails.
    ///
    pub async fn logout(&self) -> ApiServiceResult<()> {
        self.client.logout().await;
        self.store.write().await.clear().await?;

        Ok(())
    }

    /// Get the connection status of the current session.
    ///
    /// Underlying it will ping the Proton server with one second timeout to check
    /// if the connection can be established. The method will return the current
    /// status if is fresh enough without making a new request.
    ///
    /// The connection status can be one of the following:
    /// - `ConnectionStatus::Online`: The application is online and server is reachable.
    /// - `ConnectionStatus::Offline`: The application is offline.
    /// - `ConnectionStatus::ServerUnreachable`: The application is online but the server is unreachable.
    ///
    pub async fn status(&self) -> ConnectionStatus {
        self.status.status(self.client.clone()).await
    }
}

/// The parts of a session.
pub(crate) struct SessionParts {
    pub(crate) client: Proton,
    pub(crate) config: Arc<Config>,
    pub(crate) store: DynStore,
    pub(crate) status: StatusObserver,
}

impl Session {
    pub(crate) fn to_parts(&self) -> SessionParts {
        self.clone().into_parts()
    }

    pub(crate) fn into_parts(self) -> SessionParts {
        SessionParts {
            client: self.client,
            config: self.config,
            store: self.store,
            status: self.status,
        }
    }

    pub(crate) fn from_parts(parts: SessionParts) -> Self {
        Self {
            client: parts.client,
            config: parts.config,
            store: parts.store,
            status: parts.status,
        }
    }
}
