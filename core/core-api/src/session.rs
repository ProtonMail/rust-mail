#![allow(clippy::module_name_repetitions)]

use derive_more::Debug;
use muon::client::flow::ForkFlowResult;
use std::borrow::Borrow;
use std::sync::Arc;
use tokio::sync::{watch, RwLock};

use crate::auth::UserKeySecret;
use crate::connection_status::ConnectionStatus;
use crate::crypto_clock::init_server_crypto_clock;
use crate::human_verification::ChallengeObserver;
use crate::service::ApiServiceResult;
use crate::services::proton::{self, BuildError, Proton};
use crate::status_watcher::{StatusWatcher, StatusWatcherSubscriber};
use crate::store::{BoxStore, DynStore, Store, TempStore};

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
            env_id: EnvId::new_atlas(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn custom(env: impl Env) -> Self {
        Self {
            env_id: EnvId::new_custom(env),
            ..Self::default()
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

/// An API session builder.
#[must_use]
#[derive(Default)]
pub struct Builder {
    config: Config,
    store: Option<BoxStore>,
    status: Option<StatusWatcher>,
    challenge: Option<ChallengeObserver>,
}

impl Builder {
    /// Create a new session builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the session configuration.
    pub fn with_config(mut self, config: impl Borrow<Config>) -> Self {
        config.borrow().clone_into(&mut self.config);
        self
    }

    /// Set the app version (`x-pm-appversion`).
    pub fn with_app_version(mut self, app_version: impl AsRef<str>) -> Self {
        self.config.app_version = String::from(app_version.as_ref());
        self
    }

    /// Set the user agent.
    pub fn with_user_agent(mut self, user_agent: impl AsRef<str>) -> Self {
        self.config.user_agent = Some(String::from(user_agent.as_ref()));
        self
    }

    /// Set the environment to connect to.
    pub fn with_env_id(mut self, env_id: impl Borrow<EnvId>) -> Self {
        env_id.borrow().clone_into(&mut self.config.env_id);
        self
    }

    /// Use the Atlas environment.
    pub fn with_atlas_env(mut self) -> Self {
        self.config.env_id = EnvId::new_atlas();
        self
    }

    /// Use a custom environment.
    pub fn with_custom_env(mut self, env: impl Env) -> Self {
        self.config.env_id = EnvId::new_custom(env);
        self
    }

    /// Set the store to use.
    pub fn with_store(mut self, store: impl Store) -> Self {
        self.store = Some(Box::new(store));
        self
    }

    /// Set the status observer.
    pub fn with_status(mut self, status: StatusWatcher) -> Self {
        self.status = Some(status);
        self
    }

    /// Set the challenge observer.
    pub fn with_challenge(mut self, challenge: ChallengeObserver) -> Self {
        self.challenge = Some(challenge);
        self
    }

    /// Build the session from the builder.
    pub fn build(self) -> Result<Session, BuildError> {
        init_server_crypto_clock();

        let store = self.store.unwrap_or_else(TempStore::boxed);
        let status = self.status.unwrap_or_default();
        let challenge = self.challenge.unwrap_or_default();

        let config = Arc::new(self.config);
        let store = Arc::new(RwLock::new(store));
        let client = proton::build(&config, &store, status.observer(), challenge.clone())?;

        status.initialize(client.clone());

        Ok(Session {
            client,
            config,
            store,
            status,
            challenge,
        })
    }
}

/// An API session, capable of making requests to the API on behalf of a user.
#[derive(Debug, Clone)]
#[debug("Session {{ client: {client:?}, config: {config:?} }}")]
pub struct Session {
    client: Proton,
    config: Arc<Config>,
    store: DynStore,
    status: StatusWatcher,
    challenge: ChallengeObserver,
}

impl Session {
    /// Create a new session.
    ///
    /// # Errors
    ///
    /// Returns error if the API service failed to initialize.
    pub fn new() -> Result<Self, BuildError> {
        Self::builder().build()
    }

    /// Create a new session builder.
    pub fn builder() -> Builder {
        Builder::new()
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

    /// Returns status watcher
    ///
    #[must_use]
    pub fn status_watcher(&self) -> StatusWatcher {
        self.status.clone()
    }

    /// Observe changes on status via `Receiver`
    ///
    #[must_use]
    pub fn status_changes(&self) -> watch::Receiver<ConnectionStatus> {
        self.status.subscribe()
    }

    /// Hold task till connection status is back online
    ///
    pub async fn wait_for_online(&self) {
        self.status_changes().wait_for_online().await;
    }
}

/// The parts of a session.
pub(crate) struct SessionParts {
    pub(crate) config: Arc<Config>,
    pub(crate) store: DynStore,
    pub(crate) status: StatusWatcher,
    pub(crate) challenge: ChallengeObserver,
}

impl Session {
    pub(crate) fn to_parts(&self) -> (Proton, SessionParts) {
        self.clone().into_parts()
    }

    pub(crate) fn into_parts(self) -> (Proton, SessionParts) {
        let parts = SessionParts {
            config: self.config,
            store: self.store,
            status: self.status,
            challenge: self.challenge,
        };

        (self.client, parts)
    }

    pub(crate) fn from_parts(client: Proton, parts: SessionParts) -> Self {
        Self {
            client,
            config: parts.config,
            store: parts.store,
            status: parts.status,
            challenge: parts.challenge,
        }
    }
}
