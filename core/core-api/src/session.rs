use derive_more::{Debug, Deref};
use muon::client::flow::ForkFlowResult;
use muon::common::ParseEndpointErr;
use muon::env::DynEnv;
use std::borrow::Borrow;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, watch};

use crate::auth::UserKeySecret;
use crate::connection_status::ConnectionStatus;
use crate::crypto_clock::init_server_crypto_clock;
use crate::service::ApiServiceResult;
use crate::services::observability::ObservabilityManager;
use crate::services::proton::{self, BuildError, Proton};
use crate::status_watcher::StatusWatcher;
use crate::store::{BoxStore, DynStore, Store, TempStore};
use crate::verification::{DynChallengeNotifier, FailNotifier};

pub use muon::app::AppVersion;
pub use muon::common::{Endpoint, Server};
pub use muon::env::{Env, EnvId};
pub use muon::tls::TlsPinSet;

const OBSERVABILITY_BATCH_SIZE: usize = 500;

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

    /// A proxy to use.
    pub proxy: Option<String>,
}

impl Config {
    /// Create a new session config for the given environment.
    #[must_use]
    pub fn for_env(env: impl Env) -> Self {
        Self {
            env_id: EnvId::new_custom(env),
            ..Self::default()
        }
    }

    /// Create a new session config for the atlas environment.
    #[must_use]
    pub fn atlas() -> Self {
        Self {
            env_id: EnvId::new_atlas(),
            ..Self::default()
        }
    }

    /// Create a new session config for a named atlas environment.
    #[must_use]
    pub fn scientist(name: impl AsRef<str>) -> Self {
        Self {
            env_id: EnvId::new_atlas_name(name),
            ..Self::default()
        }
    }

    /// Create a new session config for a custom environment.
    ///
    /// This will create a new environment with the given server URL.
    /// This must be a valid URL, including the scheme, host, and if applicable,
    /// path and port. For example: `http://127.0.0.1:8888/api`.
    ///
    /// # Security
    ///
    /// This function is insecure because it allows the user to create a session
    /// with a custom environment. This can lead to security issues if the
    /// environment is not trusted. The user must ensure that the environment
    /// is safe to use and that the server is trusted.
    pub fn custom(url: impl AsRef<str>) -> Result<Self, ParseEndpointErr> {
        struct CustomEnv(Server);

        impl CustomEnv {
            fn new(server: impl AsRef<str>) -> Result<Self, ParseEndpointErr> {
                Ok(Self(server.as_ref().parse()?))
            }
        }

        impl Env for CustomEnv {
            fn servers(&self, _: &AppVersion) -> Vec<Server> {
                vec![self.0.clone()]
            }

            fn pins(&self, _: &Server) -> Option<TlsPinSet> {
                None
            }
        }

        Ok(Self::for_env(CustomEnv::new(url)?))
    }

    pub fn without_alternative_routing(mut self) -> Result<Self, BuildError> {
        struct CustomDirectEnv {
            servers: Vec<Server>,
            env: DynEnv,
        }

        impl CustomDirectEnv {
            fn new(config: &Config) -> Result<Self, BuildError> {
                let env = config.env_id.clone().build();
                let version = config.app_version.parse()?;
                let servers = env
                    .servers(&version)
                    .into_iter()
                    .filter(|server| server.host().is_direct())
                    .collect();

                Ok(Self { servers, env })
            }
        }

        impl Env for CustomDirectEnv {
            fn servers(&self, _: &AppVersion) -> Vec<Server> {
                self.servers.clone()
            }

            fn pins(&self, server: &Server) -> Option<TlsPinSet> {
                self.env.pins(server)
            }
        }

        self.env_id = EnvId::new_custom(CustomDirectEnv::new(&self)?);
        Ok(self)
    }

    /// Extracts the client id from the app version, which usually looks like "platform-app@version", eg.: android-mail@10.9
    #[must_use]
    pub fn get_client_id(&self) -> &str {
        self.app_version.split('@').next().unwrap_or_default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app_version: String::from("Other"),
            user_agent: None,
            env_id: EnvId::new_prod(),
            proxy: None,
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
    notifier: Option<DynChallengeNotifier>,
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

    /// Set the proxy to use.
    pub fn with_proxy(mut self, proxy: impl AsRef<str>) -> Self {
        self.config.proxy = Some(String::from(proxy.as_ref()));
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

    /// Set the challenge notifier.
    pub fn with_notifier(mut self, notifier: DynChallengeNotifier) -> Self {
        self.notifier = Some(notifier);
        self
    }

    /// Build the session from the builder.
    pub async fn build(self) -> Result<Session, BuildError> {
        init_server_crypto_clock();

        let store = self.store.unwrap_or_else(TempStore::boxed);
        let mut status = self.status.unwrap_or_default();
        let notifier = self.notifier.unwrap_or_else(FailNotifier::arced);

        let config = Arc::new(self.config);
        let store = Arc::new(RwLock::new(store));
        let client = proton::build(&config, &store, &status, notifier).await?;

        status.initialize(client.clone());

        ObservabilityManager::start(
            client.clone(),
            Duration::from_secs(60),
            OBSERVABILITY_BATCH_SIZE,
        );

        Ok(Session {
            client,
            config,
            store,
            status,
        })
    }
}

/// An API session, capable of making requests to the API on behalf of a user.
#[derive(Debug, Deref, Clone)]
#[debug("Session {{ client: {client:?}, config: {config:?} }}")]
pub struct Session {
    #[deref]
    client: Proton,
    config: Arc<Config>,
    store: DynStore,
    status: StatusWatcher,
}

impl Session {
    /// Create a new session.
    ///
    /// # Errors
    ///
    /// Returns error if the API service failed to initialize.
    pub async fn new() -> Result<Self, BuildError> {
        Self::builder().build().await
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
    /// Underlying it will ping the Proton server with two seconds timeout when
    /// the connection status is uncertain - to check if the connection can be
    /// established. The method will return the current status if it is fresh
    /// enough without making a new request.
    ///
    /// The connection status can be one of the following:
    /// - `ConnectionStatus::Online`: The application is online and server is reachable.
    /// - `ConnectionStatus::Offline`: The application is offline.
    /// - `ConnectionStatus::ServerUnreachable`: The application is online but the server is unreachable.
    ///
    pub async fn status(&self) -> ConnectionStatus {
        self.status.status(self.client.clone()).await
    }

    /// Get the connection status of the current session.
    ///
    /// It uses [`status`] method under the hood, but if it claims the connection
    /// cannot be made it will allow grace period of two seconds. It will follow logic:
    /// * If the connection is online, it will return `ConnectionStatus::Online` immediately.
    /// * If the connection is offline, it will wait for 2 seconds and return the current status.
    ///
    /// This method is useful to avoid returning `ConnectionStatus::Offline`
    /// when the connection status is uncertain.
    ///
    pub async fn graceful_status(&self) -> ConnectionStatus {
        match self.status().await {
            ConnectionStatus::Online => ConnectionStatus::Online,
            status => {
                match tokio::time::timeout(Duration::from_secs(2), self.wait_for_online()).await {
                    Ok(()) => ConnectionStatus::Online,
                    Err(_) => status,
                }
            }
        }
    }

    /// Returns a reference to the store.
    ///
    #[must_use]
    pub fn store(&self) -> &DynStore {
        &self.store
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

    /// Waits until the connection is online; if that's the case at the moment,
    /// returns immediately.
    ///
    pub async fn wait_for_online(&self) {
        // `wait_for()` returns `Err` if the channel's tx has died - this
        // shouldn't be the case here, because the channel is allowed to die
        // only after the *last* instance of status watcher is dropped, and we
        // know at least one instance must be alive as it's held within `self`.
        //
        // If this logic becomes violated, the worst that can happen is that
        // this function returns even if the network connection is actually
        // offline. This is alright, because listening on network status is
        // advisory anyway - the caller is supposed to handle potential network
        // problems on their side one way or another.

        _ = self
            .status_changes()
            .wait_for(ConnectionStatus::is_online)
            .await;
    }

    /// Waits until the connection is offline; if that's the case at the moment,
    /// returns immediately.
    ///
    pub async fn wait_for_offline(&self) {
        _ = self
            .status_changes()
            .wait_for(ConnectionStatus::is_offline)
            .await;
    }
}

/// The parts of a session.
#[derive(Clone)]
pub struct SessionParts {
    pub config: Arc<Config>,
    pub store: DynStore,
    pub status: StatusWatcher,
}

impl Session {
    #[must_use]
    pub fn to_parts(&self) -> (Proton, SessionParts) {
        self.clone().into_parts()
    }

    #[must_use]
    pub fn into_parts(self) -> (Proton, SessionParts) {
        let parts = SessionParts {
            config: self.config,
            store: self.store,
            status: self.status,
        };

        (self.client, parts)
    }

    #[must_use]
    pub fn from_parts(client: Proton, parts: SessionParts) -> Self {
        Self {
            client,
            config: parts.config,
            store: parts.store,
            status: parts.status,
        }
    }
}
