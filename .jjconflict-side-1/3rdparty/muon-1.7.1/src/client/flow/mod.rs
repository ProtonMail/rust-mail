//! Module
//! ## Flows
//! Flows allow to perform actions on a session that require a specific sequence
//! of steps.
//!
//! Mainly two types of flow exist:
//! 1. login flows
//! 2. fork flows
//!
//! ## Login
//! Login flows allow a session to authenticate to access authenticated routes
//! in Proton API. To authenticate, a session either provides information to
//! login or imports an existing session (child fork or externally managed).
//!
//! Note: [`flow::LoginFlow`](`crate::client::flow::LoginFlow`) assumes that the
//! client is un-authenticated, hence it performs a
//! [`Client::logout`](`crate::Client::logout`) if it wasn't the case.
//!
//! ### User-password login
//! This flow uses the given username and password to authenticate with the
//! Proton API. Depending on the account's security settings, a second factor
//! authentication may be required as well.
//!
//! The first step of the flow establishes a new API session by performing an
//! SRP exchange with the Proton API. If successful, information about the
//! account and the newly created session is returned in the
//! [`LoginFlowData`](`crate::client::flow::LoginFlowData`)
//! along with the client or the continuation of the flow.
//!
//! See the [login example](crate::client#Login-example).
//!
//! ### Login via fork
//! ```
//! use muon::store::{Store, StoreError};
//! use muon::client::{Auth, flow::{LoginFlow, WithSelectorFlow}};
//! use muon::{App, Client, env::EnvId, GET};
//! # fn handle_error(r : impl Into<muon::Error>) {}
//! # fn store_payload(p: Option<Vec<u8>>) {}
//! # async fn receive_selector_from_parent() -> String {"".to_owned()}
//! # tokio_test::block_on(async {
//! let env = EnvId::new_prod();
//! let app = App::new("windows-vpn@4.1.0")?;
//! let client = Client::new(app, env)?;
//! let selector = receive_selector_from_parent().await;
//! let client = match client.auth().from_fork().with_selector(selector).await {
//!     WithSelectorFlow::Ok(client, payload) => {
//!         // do something with the payload for later use
//!         store_payload(payload);
//!         // continue with the logged client
//!         client
//!     }
//!     WithSelectorFlow::Failed {client, reason} => {
//!         // handle the returned error
//!         handle_error(reason);
//!         // continue with the unauth client
//!         client
//!     }
//! };
//! # anyhow::Ok(())
//! # });
//! ```
//!
//! ### Login via fork with user code (interacts with [fork flows](#Fork))
//! ```
//! use muon::store::{Store, StoreError};
//! use muon::client::{Auth, flow::{WithCodeFlow, WithCodePollFlow}};
//! use muon::{App, Client, GET, env::EnvId};
//! struct MyPersistenceStorage;
//! impl MyPersistenceStorage {
//!     pub fn prod() -> Self { Self }
//! }
//! #[async_trait::async_trait]
//! impl Store for MyPersistenceStorage {
//!     fn env(&self) -> EnvId {
//!         EnvId::new_atlas()
//!     }
//!
//!     async fn get_auth(&self) -> Auth {
//!         Auth::None
//!     }
//!
//!     async fn set_auth(&mut self, auth: Auth) -> Result<Auth, StoreError>  {
//!         Ok(auth)
//!     }
//! }
//! # struct FlowErr;
//! # fn handle_error(r : FlowErr) {}
//! # fn store_payload(p: Option<Vec<u8>>) {}
//! # async fn receive_selector_from_parent() -> String {"".to_owned()}
//! # fn poll_flow_until_parent_enters_code(_: WithCodePollFlow) -> Result<(Client, Option<Vec<u8>>), (Client, FlowErr)> {
//! #   let store = MyPersistenceStorage::prod();
//! #   let app = App::new("windows-vpn@4.1.0").unwrap();
//! #   Ok::<(Client, Option<Vec<u8>>), (Client, FlowErr)>((Client::new(app, store).unwrap(), None))
//! # }
//! # tokio_test::block_on(async {
//! let store = MyPersistenceStorage::prod();
//! let app = App::new("windows-vpn@4.1.0")?;
//! let child = Client::new(app, store)?;
//! let child = match child.auth().from_fork().with_code().await {
//!     WithCodeFlow::Poll(flow) => {
//!         let code = flow.code().to_owned();
//!         match poll_flow_until_parent_enters_code(flow) {
//!             Ok((client, payload)) => {
//!                 // do something with the payload for later use
//!                 store_payload(payload);
//!                 // continue with the logged client
//!                 client
//!             },
//!
//!             Err((client, reason)) => {
//!                 // handle the returned error
//!                 handle_error(reason);
//!                 // continue with the unauth client
//!                 client
//!             }
//!         }
//!     }
//!     _ => anyhow::bail!("unexpected success and/or failure"),
//! };
//! # anyhow::Ok(())
//! # });
//! ```
//! ## Fork
//! Fork flows allow to fork a parent session to a child session, such that the
//! child session can be imported somewhere else via a [login flows](#Login)

use crate::client::Client;
use crate::error::Result;
use crate::http::{StatusErr, POST};
use crate::rest::auth;
use crate::rest::auth::v4::fido2;
use crate::util::{ByteSliceErr, ByteSliceExt};
use crate::{Auth, Error, ErrorKind};
use proton_srp::SRPError;
use thiserror::Error;

/// Macro absolutely INTERNAL to the flow module.
///
/// It is only there to lighten a bit the code and allow to return the Unlogged
/// state from within a flow.
macro_rules! return_variant_on_error {
    ($e:expr, $client:expr, $type:path) => {
        match $e {
            Ok(v) => v,
            Err(e) => {
                return {
                    {
                        $type {
                            client: $client,
                            reason: e.into(),
                        }
                    }
                }
            }
        }
    };
}

export! {
    mod from_fork (as pub);
    mod login (as pub);
}

/// An auth flow.
///
/// This is the entry point for authenticating with the Proton API using the
/// `muon` client. The auth flow enforces a strict sequence of steps to ensure
/// things don't go wrong.
#[must_use]
#[derive(Debug)]
pub struct AuthFlow {
    client: Client,
}

impl AuthFlow {
    pub(super) fn new(client: Client) -> Self {
        Self { client }
    }

    /// Begin the login flow.
    ///
    /// This will authenticate with the Proton API using the given
    /// username and password. If successful, a `LoginFlow` will be returned,
    /// which may contain the `Client`, or may require further steps to
    /// complete the authentication process (e.g. two-factor authentication).
    pub async fn login(self, user: impl AsRef<str>, pass: impl AsRef<str>) -> LoginFlow {
        // logout first, such that we know from which state we are logging in
        self.client.logout().await;

        // start the login flow
        LoginFlow::new(self.client, user.as_ref(), pass.as_ref()).await
    }

    /// Begin the login flow.
    ///
    /// This will authenticate with the Proton API using the given
    /// username and password and optional extra info.
    /// If successful, a `LoginFlow` will be returned,
    /// which may contain the `Client`, or may require further steps to
    /// complete the authentication process (e.g. two-factor authentication).
    ///
    /// # Example
    /// ```
    /// use serde_json::json;
    /// use muon::{App, Client};
    /// use muon::env::EnvId;
    /// use muon::client::flow::{AuthFlow, LoginExtraInfo};
    ///
    /// let app = App::new("windows-vpn@4.1.0").unwrap();
    /// let env = EnvId::new_prod();
    /// let client = Client::new(app, env).unwrap();
    ///
    /// let extra_info = LoginExtraInfo::builder()
    ///     .with_fingerprint(json!([{
    ///         "appLang": "en",
    ///         "timezone": "America/New_York",
    ///         "frame": { "name": "username" }
    ///     }]).into())
    ///     .build();
    /// let _ = client.auth().login_with_extra("user", "password", extra_info);
    /// ```
    #[deprecated(
        note = "Use login instead. For the fingerprint pass a provider to the muon client using with_info_provider. Muon will use this provider to ask for the fingerprint when needed."
    )]
    #[allow(deprecated)]
    pub async fn login_with_extra(
        self,
        user: impl AsRef<str>,
        pass: impl AsRef<str>,
        extra_info: LoginExtraInfo,
    ) -> LoginFlow {
        // logout first, such that we know from which state we are logging in
        self.client.logout().await;

        LoginFlow::new_with_extra(self.client, user.as_ref(), pass.as_ref(), extra_info).await
    }

    /// Resume the login flow from the 2FA stage using a TOTP code.
    ///
    /// This is used when an existing login flow has been interrupted before
    /// the 2FA stage could be completed. The `code` should be the 2FA code
    /// that was generated by the user's authenticator app.
    ///
    /// # Errors
    ///
    /// Returns an error if the login flow fails to resume.
    ///
    /// # Example
    ///
    /// ```
    /// use muon::{App, Client};
    /// # use muon::doc::*;
    /// # tokio_test::block_on(async {
    /// let store = MyPersistenceStorage::prod();
    /// let app = App::new("windows-vpn@4.1.0")?;
    /// let client = Client::new(app, store)?;
    /// let client = client.auth().from_totp("123456").await?;
    /// # anyhow::Ok(())
    /// # });
    /// ```
    pub async fn from_totp(self, code: impl AsRef<str>) -> Result<Client> {
        LoginTwoFactorFlow::from_totp(self.client, code).await
    }

    /// Resume the login flow from the 2FA stage using a FIDO2 device.
    ///
    /// This is used when an existing login flow has been interrupted before
    /// the 2FA stage could be completed.
    ///
    /// # Errors
    ///
    /// Returns an error if the login flow fails to resume.
    ///
    /// # Example
    ///
    /// TODO: Un-ignore this example when the FIDO2 flow is implemented.
    ///
    /// ```ignore
    /// use muon::{App, Client};
    /// # use muon::doc::*;
    /// # tokio_test::block_on(async {
    /// let store = MyPersistenceStorage::prod();
    /// let app = App::new("windows-vpn@4.1.0")?;
    /// let client = Client::new(app, store)?;
    /// let client = client.auth().from_fido("{... json ...}").await?;
    /// # anyhow::Ok(())
    /// # });
    /// ```
    pub async fn from_fido(self, fido_data: fido2::Request) -> Result<Client> {
        LoginTwoFactorFlow::from_fido(self.client, fido_data).await
    }

    /// Provide an externally managed session UID.
    ///
    /// The `muon` client will set the session UID in outgoing requests but will
    /// **not** manage the session; it is assumed that the session is
    /// managed by an external system, such as a browser. If the session's
    /// access token expires, muon will not attempt to refresh it.
    pub async fn from_uid(self, user_id: impl AsRef<str>, uid: impl AsRef<str>) -> Client {
        // Build the auth object with only the UID.
        let auth = Auth::external(user_id, uid);

        // Set the (external) auth object.
        self.client.stores.set_auth(auth).await;

        self.client
    }

    /// Authenticate by taking over a forked session.
    pub fn from_fork(self) -> FromForkFlow {
        FromForkFlow::new(self.client)
    }
}

/// Final fork flow result, it can either be a be a success an return the client
/// along with the selector or return a failure and return the client and the
/// reason.
#[derive(Debug)]
pub enum ForkFlowResult {
    /// Indicates a successful fork flow with the parent session [`Client`],
    /// the selector, and the session UID.
    Success(Client, String, Option<String>),
    /// todo
    Failure {
        /// We return the client even if it couldn't accept the fork
        client: Client,
        /// the reason why the fork failed
        reason: FlowErr,
    },
}

/// The fork flow type.
#[must_use]
#[derive(Debug)]
pub struct ForkFlow {
    client: Client,
    child: String,
    independent: bool,
    payload: Option<Vec<u8>>,
    code: Option<String>,
}

/// Extract Session-Id cookie value from response headers
fn extract_session_uid_from_headers(headers: &crate::http::Headers) -> Option<String> {
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|cookie_str| {
            // Parse the cookie string to find Session-Id
            if cookie_str.starts_with("Session-Id=") {
                // Extract the value up to the first semicolon
                let value_part = cookie_str.strip_prefix("Session-Id=")?;
                let session_uid = value_part.split(';').next()?;
                Some(session_uid.to_owned())
            } else {
                None
            }
        })
}

impl ForkFlow {
    /// Create a new fork flow from the given parts and child client identifier.
    pub(super) fn new(client: Client, child: impl AsRef<str>) -> Self {
        Self {
            client,
            child: child.as_ref().to_owned(),
            independent: false,
            payload: None,
            code: None,
        }
    }

    /// Mark the fork as independent.
    pub fn independent(mut self) -> Self {
        self.independent = true;
        self
    }

    /// Set the payload for the fork.
    pub fn payload(mut self, payload: impl AsRef<[u8]>) -> Self {
        self.payload = Some(payload.as_ref().to_owned());
        self
    }

    /// Set the user code for the fork.
    pub fn code(mut self, code: impl AsRef<str>) -> Self {
        self.code = Some(code.as_ref().to_owned());
        self
    }

    /// Perform the fork.
    pub async fn send(self) -> ForkFlowResult {
        info!(%self.child, "forking session");

        let req = auth::v4::sessions::forks::Post {
            child: self.child,
            independent: self.independent.into(),
            payload: self.payload.as_ref().map(ByteSliceExt::as_b64),
            code: self.code,
        };

        // Send the fork request.
        let req = return_variant_on_error!(
            POST!("/auth/v4/sessions/forks").body_json(&req),
            self.client,
            ForkFlowResult::Failure
        );
        let res = return_variant_on_error!(
            self.client.send(req).await,
            self.client,
            ForkFlowResult::Failure
        );

        // Parse the fork response.
        let res = return_variant_on_error!(res.ok(), self.client, ForkFlowResult::Failure);

        // Extract Session-Id cookie from response headers
        let session_uid = extract_session_uid_from_headers(res.headers());

        let res: auth::v4::sessions::forks::PostRes =
            return_variant_on_error!(res.into_body_json(), self.client, ForkFlowResult::Failure);

        ForkFlowResult::Success(self.client, res.selector, session_uid)
    }
}

mod errors {
    use super::*;

    #[derive(Debug, Error)]
    #[error("server proof mismatch")]
    pub struct SrpServerProofErr;

    #[derive(Debug, Error)]
    #[error("unexpected auth scope")]
    pub struct AuthScopeErr;

    #[derive(Debug, Error)]
    #[error("unexpected auth state")]
    pub struct AuthStateErr;

    #[derive(Debug, Error)]
    #[error("unknown user ID")]
    pub struct UserIdErr;

    #[derive(Debug, Error)]
    #[error("client flow: {0}")]
    pub enum FlowErr {
        Srp(#[from] SRPError),
        AuthScope(#[from] AuthScopeErr),
        AuthState(#[from] AuthStateErr),
        UserId(#[from] UserIdErr),
        StatusErr(#[from] StatusErr),
        ServerProof(#[from] SrpServerProofErr),
        Decode(#[from] ByteSliceErr),
        Inner(#[from] Error),
    }

    impl From<FlowErr> for Error {
        fn from(err: FlowErr) -> Self {
            if let FlowErr::Inner(err) = err {
                match err.kind() {
                    ErrorKind::Tls
                    | ErrorKind::Resolve
                    | ErrorKind::Dial
                    | ErrorKind::Connect
                    | ErrorKind::Send => err,
                    _ => err.map_kind(ErrorKind::Auth),
                }
            } else {
                ErrorKind::auth(err)
            }
        }
    }
}

use self::errors::*;
