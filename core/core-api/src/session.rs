use derive_more::Debug;
use futures::FutureExt;
use muon::client::InfoProvider;
use muon::client::flow::{ForkFlowResult, WithSelectorFlow};
use muon::common::{BoxFut, ParseEndpointErr, Sender};
use muon::rt::DynResolver;
use muon::{ProtonRequest, ProtonResponse, Result as MuonResult};
use std::borrow::Borrow;
use std::fmt::Formatter;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::UserKeySecret;
use crate::crypto_clock::init_server_crypto_clock;
use crate::service::ApiServiceResult;
use crate::services::proton::{self, BuildError};
use crate::store::{BoxStore, DynStore, Store, TempStore};
use crate::verification::{DynChallengeNotifier, FailNotifier};

pub use muon::app::AppVersion;
pub use muon::common::{Endpoint, Name, Server};
pub use muon::env::{Env, EnvId};
pub use muon::tls::TlsPinSet;
use proton_network_monitor_service::{ConnectionMonitor, NetworkStatusObserver};

pub trait EnvIdExt: Sized {
    /// Create a new environment ID for a custom environment.
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
    fn new_custom_url(url: impl AsRef<str>) -> Result<Self, ParseEndpointErr>;
}

impl EnvIdExt for EnvId {
    fn new_custom_url(url: impl AsRef<str>) -> Result<Self, ParseEndpointErr> {
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
        }

        Ok(Self::new_custom(CustomEnv::new(url)?))
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

    /// A resolver to use.
    pub resolver: Option<DynResolver>,
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
            resolver: None,
        }
    }
}

#[must_use]
pub struct Builder {
    config: Config,
    store: Option<BoxStore>,
    connection_monitor: Option<ConnectionMonitor>,
    notifier: Option<DynChallengeNotifier>,
    info_provider: Option<Arc<dyn InfoProvider>>,
    allow_doh: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            config: Config::default(),
            store: None,
            connection_monitor: None,
            notifier: None,
            info_provider: None,
            allow_doh: true,
        }
    }
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(mut self, config: impl Borrow<Config>) -> Self {
        config.borrow().clone_into(&mut self.config);
        self
    }

    pub fn with_app_version(
        mut self,
        platform: impl AsRef<str>,
        product: impl AsRef<str>,
        version: impl AsRef<str>,
    ) -> Self {
        self.config.app_version =
            format_api_app_version(platform.as_ref(), product.as_ref(), version.as_ref());
        self
    }

    pub fn with_user_agent(mut self, user_agent: impl AsRef<str>) -> Self {
        self.config.user_agent = Some(String::from(user_agent.as_ref()));
        self
    }

    pub fn with_env_id(mut self, env_id: impl Borrow<EnvId>) -> Self {
        env_id.borrow().clone_into(&mut self.config.env_id);
        self
    }

    pub fn with_proxy(mut self, proxy: impl AsRef<str>) -> Self {
        self.config.proxy = Some(String::from(proxy.as_ref()));
        self
    }

    pub fn with_atlas_env(mut self) -> Self {
        self.config.env_id = EnvId::new_atlas();
        self
    }

    pub fn with_store(mut self, store: impl Store) -> Self {
        self.store = Some(Box::new(store));
        self
    }

    pub fn with_connection_monitor(mut self, monitor: ConnectionMonitor) -> Self {
        self.connection_monitor = Some(monitor);
        self
    }

    pub fn with_notifier(mut self, notifier: DynChallengeNotifier) -> Self {
        self.notifier = Some(notifier);
        self
    }

    pub fn with_info_provider(mut self, info_provider: Arc<dyn InfoProvider>) -> Self {
        self.info_provider = Some(info_provider);
        self
    }

    pub fn with_allow_doh(mut self, value: bool) -> Self {
        self.allow_doh = value;
        self
    }

    pub async fn build(self) -> Result<Session, BuildError> {
        init_server_crypto_clock();

        let store = self.store.unwrap_or_else(TempStore::boxed);
        let connection_monitor = self.connection_monitor.unwrap_or_else(|| {
            tracing::warn!("Creating connection monitor in standalone mode");
            ConnectionMonitor::standalone()
        });
        let notifier = self.notifier.unwrap_or_else(FailNotifier::arced);
        let config = Arc::new(self.config);
        let store = Arc::new(RwLock::new(store));

        let client = proton::build(
            &config,
            &store,
            notifier,
            self.info_provider,
            self.allow_doh,
        )
        .await?;

        Ok(Session {
            client,
            config,
            store,
            connection_monitor: connection_monitor.clone(),
            network_status_observer: connection_monitor.network_status_observer(),
        })
    }
}

/// An API session, capable of making requests to the API on behalf of a user.
#[derive(Clone)]
pub struct Session {
    client: muon::Client,
    config: Arc<Config>,
    store: DynStore,
    connection_monitor: ConnectionMonitor,
    network_status_observer: NetworkStatusObserver,
}

impl Sender<ProtonRequest, ProtonResponse> for Session {
    fn send(&self, req: ProtonRequest) -> BoxFut<'_, MuonResult<ProtonResponse>> {
        self.send_impl(req).boxed()
    }
}

impl Session {
    async fn send_impl(&self, req: ProtonRequest) -> MuonResult<ProtonResponse> {
        let result = self.client.send(req).await;
        self.connection_monitor.inspect_result(&result);
        result
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Session {{ client: {:?}, config: {:?} }}",
            self.client, self.config
        )
    }
}

impl Session {
    pub async fn new() -> Result<Self, BuildError> {
        Self::builder().build().await
    }

    pub fn builder() -> Builder {
        Builder::new()
    }

    /// Fork the current session for a child with the given platform and product.
    /// If successful, returns a selector by which the child can acquire the new
    /// session. The child must present an app version that matches the platform
    /// and product.
    ///
    pub async fn fork(
        &self,
        platform: impl AsRef<str>,
        product: impl AsRef<str>,
    ) -> ApiServiceResult<String> {
        let platform = platform.as_ref();
        let product = product.as_ref();
        let version = format!("{platform}-{product}");

        match self.client.clone().fork(version).send().await {
            ForkFlowResult::Success(_, selector) => Ok(selector),
            ForkFlowResult::Failure { reason, .. } => Err(muon::Error::from(reason))?,
        }
    }

    /// It takes exsiting session and downgrades it to a child session.
    ///
    /// Note: Should be used only in a case where `store` is set to `TempStore`.
    /// Otherwise it may cause uniqueness error in the database (only one core session per user) when
    /// storing the session to DB.
    pub async fn downgrade_to_fork(
        self,
        platform: impl AsRef<str>,
        product: impl AsRef<str>,
    ) -> ApiServiceResult<Self> {
        let platform = platform.as_ref();
        let product = product.as_ref();
        tracing::info!(%platform, %product, "Downgrading session to fork");
        let selector = self.fork(platform, product).await?;
        let flow = self
            .client
            .clone()
            .auth()
            .from_fork()
            .with_selector(selector)
            .await;

        match flow {
            WithSelectorFlow::Ok(client, _payload) => Ok(Self {
                client,
                config: self.config.clone(),
                store: self.store.clone(),
                connection_monitor: self.connection_monitor.clone(),
                network_status_observer: self.network_status_observer.clone(),
            }),
            WithSelectorFlow::Failed { reason, .. } => Err(muon::Error::from(reason).into()),
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
    pub async fn logout(&self) -> ApiServiceResult<()> {
        self.client.logout().await;
        self.store.write().await.clear_session().await?;

        Ok(())
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
    pub fn network_status_observer(&self) -> NetworkStatusObserver {
        self.network_status_observer.clone()
    }
}

/// The parts of a session.
#[derive(Clone)]
pub struct SessionParts {
    pub config: Arc<Config>,
    pub store: DynStore,
    pub connection_monitor: ConnectionMonitor,
    pub network_status_observer: NetworkStatusObserver,
}

impl Session {
    #[must_use]
    pub fn to_parts(&self) -> (muon::Client, SessionParts) {
        self.clone().into_parts()
    }

    #[must_use]
    pub fn into_parts(self) -> (muon::Client, SessionParts) {
        let parts = SessionParts {
            config: self.config,
            store: self.store,
            connection_monitor: self.connection_monitor,
            network_status_observer: self.network_status_observer,
        };

        (self.client, parts)
    }

    #[must_use]
    pub fn from_parts(client: muon::Client, parts: SessionParts) -> Self {
        Self {
            client,
            config: parts.config,
            store: parts.store,
            connection_monitor: parts.connection_monitor,
            network_status_observer: parts.network_status_observer,
        }
    }
}

#[must_use]
pub fn format_api_app_version(platform: &str, product: &str, version: &str) -> String {
    format!("{platform}-{product}@{version}")
}
