//! ## Client
//!
//! A client is the main component of `Muon`. It allows to communicate with the
//! Proton API by submitting Proton API queries and receiving Proton API
//! responses.
//!
//! The client:
//! - manages its own authentication, refreshing OAuth tokens if needed
//! - ensures that a query is sent by any possible means within user imposed
//!   constraints and fail only if all possible scenarios have been tried. It
//!   means that the client ensures that the query was retried and tried
//!   multiple path to reach the API if needed.
//! - is shareable and cheaply clonable. It means that the client handles
//!   concurrent access to its shared resources
//!
//! ## Create a client without persistent storage
//!
//! A client needs at least an [`App`](`crate::App`) and an
//! [`EnvId`](`crate::env::EnvId`).
//!
//! ```
//! use muon::{App, Client, env::EnvId, GET};
//! # use muon::doc::*;
//! # tokio_test::block_on(async {
//! let app = App::new("windows-vpn@4.1.0")?;
//! let env = EnvId::new_prod();
//! let client = Client::new(app, env)?;
//! let res = client.send(GET!("/tests/ping")).await?;
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! ## Create a client with persistent storage
//! To have a persistent storage, one should provide an implementation of
//! [`Store`](`crate::store::Store`) along with an [`App`](`crate::App`).
//!
//! ### Example
//! ```
//! #
//! # use muon::doc::*;
//! use muon::{App, Client, GET};
//! # tokio_test::block_on(async {
//! let store = MyPersistenceStorage::prod();
//! let app = App::new("windows-vpn@4.1.0")?;
//! let client = Client::new(app, store)?;
//! let res = client.send(GET!("/tests/ping")).await?;
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! ## Client interface
//! A [`Client`](`crate::Client`) exposes the interface to:
//! - Authenticate: [`Client::auth`](`crate::Client::auth`) with the associated
//!   flows [`client::flow::AuthFlow`](`crate::client::flow::AuthFlow`). See
//!   [`client::flow`](`crate::client::flow`) for authentication details.
//!
//! ### Login example
//! ```
//! use muon::client::flow::LoginFlow;
//! use muon::client::PasswordMode;
//! use muon::{App, Client, GET};
//! use async_trait::async_trait;
//! use muon::client::{Fingerprint, InfoProvider};
//! use serde_json::json;
//! use std::sync::Arc;
//! # use muon::doc::*;
//! # tokio_test::block_on(async {
//! const USER: &str = "user";
//! const PASS: &str = "pass";
//!
//! // InfoProviderImpl is created by the client apps and passed to muon
//! // It takes care of requesting extra info from the clients on a need basis
//! // Including requesting the fingerprint as you can see below:
//! struct InfoProviderImpl {}
//! #[async_trait]
//! impl InfoProvider for InfoProviderImpl {
//!    async fn fingerprint(&self) -> Option<Fingerprint> {
//!        let fingerprint = json!({
//!            "mail-android-99.9.40.0-challenge":{
//!                "appLang":"en",
//!                "deviceName":"TestDevice",
//!                "frame":{
//!                    "name":"username"
//!                },
//!                "isDarkmodeOn":false,
//!                "isJailbreak":false,
//!                "keyboards":[
//!
//!                ],
//!                "preferredContentSize":"2.0",
//!                "regionCode":"CH",
//!                "storageCapacity":"63.8",
//!                "timezone":"Europe/Zurich",
//!                "timezoneOffset":"0",
//!                "v":"2.0.0"
//!            }
//!        })
//!        .into();
//!        Some(fingerprint)
//!    }
//! }
//!
//! let store = MyPersistenceStorage::prod();
//! let app = App::new("windows-vpn@4.1.0")?;
//! // Create the client with an info_provider. The info_provider requests additional information from the client on demand, including the fingerprint.
//! // The fingerprint is needed, for example, when sending a call to create an unauthenticated session, or during login.
//! let client = Client::new(app, store)?.with_info_provider(Arc::new(InfoProviderImpl {}));
//!
//! // perform the login flow
//! let (client, data) = match client.auth().login(USER, PASS).await {
//!     LoginFlow::Ok(authenticated_client, data) => {
//!         // show that we successfully logged in
//!         display_authenticated_user_info(&authenticated_client);
//!
//!         // continue with the client and the associated data
//!         (authenticated_client, Some(data))
//!     },
//!
//!     // The user needs to provide 2FA
//!     LoginFlow::TwoFactor(client_needs_2fa, data) => {
//!         // show a modal asking for 2fa
//!         let twofa = ask_user_for_2fa();
//!
//!         // provide the 2fa, we either get an authenticated client or not.
//!         (client_needs_2fa.totp(twofa).await?, Some(data))
//!     },
//!
//!     LoginFlow::Failed {client, reason} => {
//!         // handle the error
//!         show_user_cant_login_modal(reason);
//!
//!         // continue with the unauthenticated client
//!         (client, None)
//!     },
//! };
//!
//! // ensure that we have authenticated data
//! let Some(data) = &data else {
//!     anyhow::bail!("authentication failed");
//! };
//!
//! // load the preferences of the user using its user-id
//! load_user_preferences(&data.user_id)?;
//!
//! // get the password for the user's PGP key (if needed)
//! let password = match data.password_mode {
//!     PasswordMode::One => PASS.to_owned(),
//!     PasswordMode::Two => ask_user_for_mbp(),
//! };
//!
//! // unlock the user's PGP key with the correct password
//! unlock_pgp_key(&client, &password);
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! - logout: [`Client::logout`](`crate::Client::logout`) can never fail as the
//!   local state must always be editable. If there is an error during the
//!   logout process, the application should behave as un-authenticated, and
//!   behave as-is until instructed otherwise by the user (i.e., login). In case
//!   of state inconsistency, the best thing to do is to behave normally (we do
//!   not guess the state) and the API will tell us what to do.
//!
//! ### Logout example
//! ```
//! use muon::{App, Client, env::EnvId};
//! # use muon::doc::*;
//! # tokio_test::block_on(async {
//! let env = EnvId::new_atlas();
//! let app = App::new("windows-vpn@4.1.0")?;
//! let client = Client::new(app, env)?;
//!
//! // skipping the login part ...
//!
//! client.logout().await;
//!
//! // the client is now un-authenticated
//!
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! - Fork: [`fork`](crate::Client::fork)
//!
//!   see [`flow`] for details forking and import forks.
//!
//! ### Fork a parent session example
//! ```
//! # use muon::doc::*;
//! # fn display_selector() -> String {"".to_owned()}
//! # fn handle_fork_failure(r: impl Into<muon::Error>) {}
//! # async fn send_selector_to_slave(s: String) {}
//! # tokio_test::block_on(async {
//! use muon::client::flow::ForkFlowResult;
//! use muon::{App, Client, GET};
//! let store = MyPersistenceStorage::prod();
//! let app = App::new("windows-vpn@4.1.0")?;
//! let client = Client::new(app, store)?;
//!
//! // skipping the login part ...
//! let client = match client
//!     .fork("windows-vpn")
//!     .payload(b"hello world")
//!     .send()
//!     .await {
//!     // Client has been forked successfully ...
//!     ForkFlowResult::Success(client, selector) => {
//!         // send the selector to the slave
//!         send_selector_to_slave(selector).await;
//!         // continue normally with the client
//!         client
//!     }
//!     ForkFlowResult::Failure { client, reason } => {
//!         // handle the fork failure; e.g., too many forks or recursive fork
//!         handle_fork_failure(reason);
//!         // continue with the un-forked client
//!         client
//!     },
//!
//! };
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! - Request sending: [`Client::send`](`crate::Client::send`)
//!
//! ### Example
//! ```
//! #
//! # use muon::doc::*;
//! # tokio_test::block_on(async {
//! use muon::{App, Client, GET};
//! let store = MyPersistenceStorage::prod();// aa
//! let app = App::new("windows-vpn@4.1.0")?;
//! let client = Client::new(app, store)?;
//! let res = client.send(GET!("/tests/ping")).await?;
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! ## Session state storage
//! A [`Client`](`crate::Client`) contains an in-memory storage and a user
//! defined persistence storage following the
//! [`store::Store`](`crate::store::Store`) interface. Both storage are
//! infallible, the in-memory storage is the source of truth for the
//! [`Client`](`crate::Client`).
//!
//! In case the persistence storage can not be synchronized
//! with the local one (e.g., due to IO errors), the state IS considered
//! inconsistent and the application is considered on its own. The persistent
//! storage is assumed to handle his errors on its own (see
//! example-fallible-store).
//!
//! ### How to resync an de-synchronized store
//! ```
//! # use muon::doc::*;
//! # fn is_store_unsync() -> bool { true }
//! # fn display_modal_unsync() {}
//! # tokio_test::block_on(async {
//! use std::thread;
//! use std::time::Duration;
//! use muon::{App, Client, GET};
//! use muon::client::flow::LoginFlow;
//! let store = MyPersistenceStorage::prod();
//! let app = App::new("windows-vpn@4.1.0")?;
//! let client = Client::new(app, store)?;
//! let client = match client.auth().login("user", "password").await {
//!     LoginFlow::Ok(authenticated_client, _) => {
//!         // show that we successfully logged in
//!         display_authenticated_user_info(&authenticated_client);
//!         // continue with the client
//!         authenticated_client
//!     },
//!     // The user needs to provide 2FA
//!     LoginFlow::TwoFactor(client_needs_2fa, _) => {
//!         // show a modal asking for 2fa
//!         let twofa = ask_user_for_2fa();
//!         // provide the 2fa, we either get an authenticated client or not.
//!         client_needs_2fa.totp(twofa).await?
//!     },
//!     LoginFlow::Failed {client, reason} => {
//!         // handle the error
//!         show_user_cant_login_modal(reason);
//!         // continue with the unauthenticated client
//!         client
//!     },
//! };
//!
//! // check if we have received a signal that we have been unsync
//! if is_store_unsync() {
//!     // show a modal telling the user how to unblock the situation
//!     display_modal_unsync();
//!     // wait for the user to resolve
//!     thread::sleep(Duration::from_secs(5));
//!     // manually try to sync again
//!     client.sync_stores().await;
//! }
//!
//! # anyhow::Ok(())
//! # });
//! ```

use crate::app::App;
use crate::client::flow::{AuthFlow, ForkFlow};
use crate::common::prelude::*;
use crate::env::EnvId;
use crate::error::{ErrorKind, Result};
use crate::http::{DynHttpSender, HttpReq, HttpRes, DELETE};
use crate::middleware::AuthLayer;
use async_trait::async_trait;
use futures::channel::mpsc::unbounded;
use futures::{executor, TryFutureExt};
use muon_proc::derive_dyn;
use private::ClientInternalStorage;
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;

pub mod flow;
/// todo
pub mod headers;
/// todo
pub mod middleware;
// re-export of auth here
pub use crate::auth::{Auth, PasswordMode, Tokens};
/// Implements a builder for configuring a `muon` client.
mod builder;
pub use builder::Builder;

/// Helper traits for async conversions.
mod helpers;
pub(crate) use helpers::*;

mod private {
    use super::{AsyncFrom, Auth};
    use crate::env::EnvId;
    use crate::store::{AuthVersion, InMemoryStore, SafeStore, Store};
    use async_trait::async_trait;
    use futures::FutureExt as _;
    /// The stores contained in a [`Client`].
    /// It contains an in-memory store that can never fail and has the local
    /// state of the [`Client`] and a persistent one.
    #[derive(Debug, Clone)]
    pub struct ClientInternalStorage {
        /// The local in-memory store
        local_store: SafeStore,

        /// An optional persistent store. The local store will always try to
        /// push its state into the persistent store.
        persistent_store: Option<SafeStore>,
    }

    #[async_trait]
    impl AsyncFrom<EnvId> for ClientInternalStorage {
        async fn from(env_id: EnvId) -> Self {
            Self {
                local_store: SafeStore::new(InMemoryStore::new(env_id, None)),
                persistent_store: None,
            }
        }
    }

    #[async_trait]
    impl<T: Store> AsyncFrom<T> for ClientInternalStorage {
        async fn from(store: T) -> Self {
            let env_id = store.env();
            let auth = store.get_auth().await;

            Self {
                local_store: SafeStore::new(InMemoryStore::new(env_id, Some(auth))),
                persistent_store: Some(SafeStore::new(store)),
            }
        }
    }

    impl ClientInternalStorage {
        /// Get the env the [`Store`] are bound to
        pub(crate) fn env(&self) -> &EnvId {
            self.local_store.env()
        }

        /// Get a reference to the local storage
        pub(crate) fn local(&self) -> &SafeStore {
            &self.local_store
        }

        /// A convenience function that retrieves the local store's auth state.
        pub(crate) async fn get_auth(&self) -> (AuthVersion, Auth) {
            self.local_store.get_auth().await
        }

        /// A convenience function that sets the local store's auth state
        /// and attempts to sync the local store with the persistent store.
        pub(crate) async fn set_auth(&self, auth: Auth) {
            self.local_store.set_auth(auth).await;
            self.sync_stores().await;
        }

        /// Sync the local store with the persistent store
        /// There is no guarantee that the persistent store is accepting the
        /// data from the point of view of this function.
        pub(crate) async fn sync_stores(&self) {
            if let Some(persistent_store) = self.persistent_store.as_ref() {
                info!("pushing from local to persistent storage");

                // Push the auth from the local store to the persistent store;
                // we don't care about the new version.
                self.local_store
                    .get_auth()
                    .then(|(_, auth)| persistent_store.set_auth(auth))
                    .await;
            }
        }
    }
}

/// A Proton API client.
///
/// The client is the primary interface for interacting with the Proton API.
/// Internally, the client is simply a wrapper around an HTTP sender and a
/// handle to an auth store.
///
/// The client is designed to be cheaply cloneable and shareable across threads.
#[derive(Debug, Clone)]
pub struct Client {
    sender: DynHttpSender,
    stores: ClientInternalStorage,
    provider: Option<Arc<dyn InfoProvider>>,
}

impl Client {
    /// Creates a new `muon` client with a default configuration.
    /// The client will be configured for the given `app` and `store`.
    ///
    /// This is a convenience function; non-default configurations can be built
    /// using the [`Client::builder`] method directly.
    ///
    /// # Errors
    ///
    /// Returns an error if the client cannot be built, which can occur if TLS
    /// configuration fails, for example.
    pub fn new(app: App, store: impl AsyncInto<ClientInternalStorage>) -> Result<Client> {
        Self::builder(app, store).build()
    }

    /// A non-blocking variant of [`Client::new`].
    ///
    /// # Errors
    ///
    /// See [`Client::new`].
    pub async fn new_async(
        app: App,
        store: impl AsyncInto<ClientInternalStorage>,
    ) -> Result<Client> {
        Self::builder_async(app, store).await.build()
    }

    /// Creates a new client builder.
    pub fn builder(app: App, store: impl AsyncInto<ClientInternalStorage>) -> Builder {
        Builder::new(app, executor::block_on(store.into()))
    }

    /// A non-blocking variant of [`Client::builder`].
    pub async fn builder_async(app: App, store: impl AsyncInto<ClientInternalStorage>) -> Builder {
        Builder::new(app, store.into().await)
    }

    /// Add an info provider to the client. Used by muon to ask for information.
    pub fn with_info_provider(mut self, provider: Arc<dyn InfoProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Get the environment to which this client is bound.
    pub fn env(&self) -> &EnvId {
        self.stores.env()
    }

    /// Send the given request, returning the response.
    ///
    /// The request is set to expire after [`HttpReq::allowed_time`].
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails to be sent.
    pub async fn send(&self, req: HttpReq) -> Result<HttpRes> {
        let layer = match &self.provider {
            Some(p) => AuthLayer::new(self.stores.clone()).with_info_provider(p.clone()),
            None => AuthLayer::new(self.stores.clone()),
        };

        let (tx, rx) = unbounded();
        let sender = self.sender.clone().layer([layer]);
        let expiry = req.get_allowed_time();

        sender
            .send(req.extension(TimeoutCtl::new(tx)))
            .with_timeout_ctl(expiry, rx)
            .map_err(ErrorKind::send)
            .await?
    }
}

impl Client {
    /// Begin an auth flow.
    ///
    /// The auth flow enables the client to authenticate with the Proton API.
    pub fn auth(self) -> AuthFlow {
        AuthFlow::new(self)
    }

    /// Begin a fork flow.
    ///
    /// The fork flow enables the client to fork its auth session to a child.
    pub fn fork(self, client: impl AsRef<str>) -> ForkFlow {
        ForkFlow::new(self, client)
    }

    /// Begin a logout flow.
    ///
    /// The logout flow deletes the client's session from the Proton API.
    ///
    /// ```
    /// use muon::{App, Client};
    /// use muon::env::EnvId;
    /// use muon::client::flow::LoginFlow;
    /// let app = App::new("windows-vpn@4.1.0").unwrap();
    /// let env = EnvId::new_prod();
    /// let client = Client::new(app, env).unwrap();
    /// let _ = async {
    ///     let client = match client.auth().login("user", "pass").await {
    ///         LoginFlow::Ok(client, _) => client,
    ///         LoginFlow::TwoFactor(_, _) => panic!("Second factor required."),
    ///         LoginFlow::Failed { reason, .. } => panic!("Login failed: {}.", reason),
    ///     };
    ///     assert!(client.is_authenticated().await);
    ///     client.logout().await;
    ///     assert!(!client.is_authenticated().await);
    /// };
    /// ```
    pub async fn logout(&self) {
        let layer = AuthLayer::new(self.stores.clone());
        let sender = self.sender.clone();

        if !self.is_authenticated().await {
            info!("client is already unlogged");
            return;
        }

        debug_assert!(self.is_authenticated().await);

        info!("deleting session");
        let _ = sender.layer([layer]).send(DELETE!("/auth/v4")).await;

        info!("clearing store");
        self.stores.local().set_auth(Auth::None).await;
        self.stores.sync_stores().await;

        info!("auth session deleted");

        debug_assert!(!self.is_authenticated().await);
    }
}

impl Client {
    /// Tells if the client is authenticated or not
    ///
    /// ```ignore
    /// let app = App::new("windows-vpn@4.1.0")?;
    /// let app = app.with_user_agent("Mozilla/5.0");
    /// let store = MyPersistentStore::prod();
    /// let client = Client::new(app, store)?;
    /// assert!(!client.is_authenticated);
    /// ```
    pub async fn is_authenticated(&self) -> bool {
        !matches!(self.stores.get_auth().await, (_, Auth::None))
    }

    /// Manually try to synchronize the local store with the persistent
    /// storage(s).
    ///
    /// It will push the local state of the client held in the in-memory storage
    /// into the registered persistent storage(s).
    ///
    /// The sync may or may not fail, but it is the responsibility of the
    /// user-defined [`Store`](crate::store::Store) to handle them.
    /// ```
    /// # use muon::doc::*;
    /// # use std::time::Duration;
    /// # use std::thread;
    /// # fn is_store_unsync() -> bool { true }
    /// # fn display_modal_unsync() {}
    /// # tokio_test::block_on(async {
    /// # use muon::{App, Client, http::GET};
    /// let app = App::new("windows-vpn@4.1.0")?;
    /// let app = app.with_user_agent("Mozilla/5.0");
    /// let store = MyPersistenceStorage::prod();
    /// let client = Client::new(app, store)?;
    /// // ... skip the login part ...
    /// client.logout().await;
    /// // storage error detected ... wait until resolved...
    /// if is_store_unsync() {
    ///   thread::sleep(Duration::from_secs(10));
    ///   // try to re-sync manually
    ///   client.sync_stores().await;
    /// }
    /// # anyhow::Ok(())
    /// # });
    /// ```
    pub async fn sync_stores(&self) {
        self.stores.sync_stores().await
    }
}

impl Client {
    fn from_parts(sender: DynHttpSender, stores: ClientInternalStorage) -> Self {
        Self {
            sender,
            stores,
            provider: None,
        }
    }
}

impl Sender<HttpReq, HttpRes> for Client {
    fn send(&self, req: HttpReq) -> BoxFut<'_, Result<HttpRes>> {
        Box::pin(self.send(req))
    }
}

/// An interface to the info provider. This is used by the muon Client to ask
/// for extra info from the apps that use it. All functions defined by
/// InfoProvider need return optionals. Apps that integrate muon need to be able
/// to choose if they send the info that muon wants to request.
/// Here's an example on how to use InfoProvider
/// ```
/// use muon::{App, Client};
/// use muon::env::EnvId;
/// use muon::client::flow::LoginFlow;
/// use std::sync::Arc;
/// use async_trait::async_trait;
/// use muon::client::{InfoProvider,Fingerprint};
/// use serde_json::json;
///
/// // ExampleInfoProvider shows how to implement the InfoProvider trait a client that uses muon.
/// struct ExampleInfoProvider {}
/// #[async_trait]
/// impl InfoProvider for ExampleInfoProvider {
///     async fn fingerprint(&self) -> Option<Fingerprint> {
///         let fingerprint = json!({
///             "mail-android-99.9.40.0-challenge":{
///                 "appLang":"en",
///                 "deviceName":"TestDevice",
///                 "frame":{
///                     "name":"username"
///                 },
///                 "isDarkmodeOn":false,
///                 "isJailbreak":false,
///                 "keyboards":[
///                 ],
///                 "preferredContentSize":"2.0",
///                 "regionCode":"CH",
///                 "storageCapacity":"63.8",
///                 "timezone":"Europe/Zurich",
///                 "timezoneOffset":"0",
///                 "v":"2.0.0"
///             }
///         })
///         .into();
///         Some(fingerprint)
///     }
/// }
///
/// let app = App::new("windows-vpn@4.1.0").unwrap();
/// let env = EnvId::new_prod();
/// // Here we pass in the ExampleInfoProvider using with_info_provider.
/// let client = Client::new(app, env).unwrap().with_info_provider(Arc::new(ExampleInfoProvider {}));
/// let _ = async {
///     let client = match client.auth().login("user", "pass").await {
///         LoginFlow::Ok(client, _) => client,
///         LoginFlow::TwoFactor(_, _) => panic!("Second factor required."),
///         LoginFlow::Failed { reason, .. } => panic!("Login failed: {}.", reason),
///     };
///     assert!(client.is_authenticated().await);
///     client.logout().await;
///     assert!(!client.is_authenticated().await);
/// };
/// ```
#[async_trait]
#[derive_dyn(Debug)]
pub trait InfoProvider: Send + Sync + 'static {
    /// Function to provide the fingerprint.
    /// The format of the fingerprint is the responability of the clients that
    /// implement this trait. This fingerprint is used for the unauth
    /// session api call and the login calls. This method allows you to
    /// generate the fingerpring exactly as you'd like. You can use remote
    /// resources, poll you environment, etc. for pulling data in the
    /// fingerprint ATTENTION: The fingerprint call halts requests from
    /// reaching the server. If the fingerprint call takes too long the requests
    /// will time out. Make sure the fingerprint function returns quickly.
    /// Consider caching the fingerprint and returning it immediately upon
    /// request.
    async fn fingerprint(&self) -> Option<Fingerprint>;
}

/// Fingerprint to be used for anti-abuse.
#[must_use]
#[derive(Clone, Debug, Default)]
pub struct Fingerprint(Value);

impl From<Value> for Fingerprint {
    fn from(value: Value) -> Self {
        Fingerprint(value)
    }
}

impl FromStr for Fingerprint {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        s.parse::<Value>().map(Fingerprint::from)
    }
}

#[cfg(test)]
mod tests {
    use super::{BoxFut, Sender};
    use crate::client::Timeout;
    use crate::http::{HttpReq, HttpRes};
    use crate::ErrorKind;
    use std::time::Duration;

    struct MockedSender;

    impl Sender<HttpReq, HttpRes> for MockedSender {
        fn send(&self, req: HttpReq) -> BoxFut<'_, crate::Result<HttpRes>> {
            Box::pin(async move {
                futures_timer::Delay::new(req.get_allowed_time()).await;
                Err(crate::Error::new(ErrorKind::Send, Some(Timeout)))
            })
        }
    }

    #[test]
    fn test_request_expiration() {
        let delay_fut = futures_timer::Delay::new(Duration::from_secs(5));
        let req = HttpReq::new(crate::http::Method::GET, "/tests/ping")
            .allowed_time(Duration::from_secs(1));

        futures::executor::block_on(async move {
            match futures::future::select(MockedSender.send(req), delay_fut).await {
                futures::future::Either::Left((_, _)) => {}
                futures::future::Either::Right((_, _)) => panic!("the request should expire first"),
            }
        })
    }

    #[test]
    fn test_request_no_expire() {
        let delay_fut = futures_timer::Delay::new(Duration::from_secs(1));
        let req = HttpReq::new(crate::http::Method::GET, "/tests/ping")
            .allowed_time(Duration::from_secs(5));

        futures::executor::block_on(async move {
            match futures::future::select(MockedSender.send(req), delay_fut).await {
                futures::future::Either::Left((_, _)) => {
                    panic!("the request should not expire first")
                }
                futures::future::Either::Right((_, _)) => {}
            }
        })
    }
}
