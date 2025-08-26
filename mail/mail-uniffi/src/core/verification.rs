use super::datatypes::AppDetails;
use crate::errors::ProtonError;
use crate::errors::unexpected::UnexpectedError;
use crate::{core::datatypes::ApiConfig, uniffi_async};
use futures::{FutureExt, TryFutureExt};
use itertools::Itertools;
use proton_core_api::verification as hv;
use proton_mail_common::errors::ProtonMailError as RealProtonMailError;
use proton_mail_common::errors::api_service_error::UserApiServiceError as RealUserApiServiceError;
use proton_mail_common::errors::unexpected::Unexpected;
use std::{ops::Deref, sync::Arc};
use tracing::error;

pub type DynChallengeNotifier = Arc<dyn ChallengeNotifier>;

#[derive(Debug, uniffi::Record)]
pub struct Query {
    pub key: String,
    pub val: Option<String>,
}

impl From<Query> for (String, Option<String>) {
    fn from(q: Query) -> Self {
        (q.key, q.val)
    }
}

impl From<(String, Option<String>)> for Query {
    fn from((key, val): (String, Option<String>)) -> Self {
        Self { key, val }
    }
}

#[derive(Debug, uniffi::Record)]
pub struct Header {
    pub key: String,
    pub val: String,
}

impl From<Header> for (String, String) {
    fn from(h: Header) -> Self {
        (h.key, h.val)
    }
}

impl From<(String, String)> for Header {
    fn from((key, val): (String, String)) -> Self {
        Self { key, val }
    }
}

/// The server of a human verification challenge.
#[derive(Debug, uniffi::Object)]
pub struct ChallengeServer {
    inner: hv::ChallengeServer,
}

impl ChallengeServer {
    fn new(inner: hv::ChallengeServer) -> Arc<Self> {
        Arc::new(Self { inner })
    }
}

#[uniffi_export]
impl ChallengeServer {
    /// The scheme of the server.
    #[must_use]
    pub fn scheme(&self) -> String {
        self.inner.server.scheme().to_string()
    }

    /// The original hostname of the server.
    #[must_use]
    pub fn original_host(&self) -> String {
        self.inner.server.name().to_string()
    }

    /// The resolved hostname of the server.
    #[must_use]
    pub fn resolved_host(&self) -> String {
        self.inner.name.to_string()
    }

    /// The port of the server.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.inner.server.port()
    }

    /// The path of the server.
    #[must_use]
    pub fn path(&self) -> String {
        self.inner.server.path.clone()
    }

    /// Whether alternative routing is enabled.
    #[must_use]
    pub fn doh(&self) -> bool {
        self.original_host() != self.resolved_host()
    }
}

/// The payload of a human verification challenge.
#[derive(Debug, uniffi::Object)]
pub struct ChallengePayload {
    inner: hv::ChallengePayload,
}

impl ChallengePayload {
    fn new(inner: hv::ChallengePayload) -> Arc<Self> {
        Arc::new(Self { inner })
    }
}

#[uniffi_export]
impl ChallengePayload {
    /// The initial human verification token.
    #[must_use]
    pub fn token(&self) -> String {
        self.inner.token.clone()
    }

    /// The verification methods available.
    #[must_use]
    pub fn methods(&self) -> Vec<String> {
        self.inner.methods.clone()
    }

    /// The challenge description.
    #[must_use]
    pub fn description(&self) -> String {
        self.inner.description.clone()
    }

    // The challenge expiration time.
    #[must_use]
    pub fn expires_at(&self) -> u64 {
        self.inner.expires_at
    }

    /// The URL base.
    #[must_use]
    pub fn base(&self) -> String {
        self.inner.base()
    }

    /// The URL path.
    #[must_use]
    pub fn path(&self) -> String {
        self.inner.path().to_owned()
    }

    /// The query parameters of the URL.
    #[must_use]
    pub fn query(&self) -> Vec<Query> {
        self.inner.query().into_iter().map_into().collect()
    }
}

/// The response to a human verification challenge.
#[derive(Debug, uniffi::Enum)]
pub enum ChallengeResponse {
    /// The challenge was successfully completed.
    Success {
        /// The token to submit to the server.
        token: String,
        /// The type of the token.
        ttype: String,
    },

    /// The challenge was not completed.
    Failure,

    /// The challenge was cancelled.
    Cancelled,
}

impl From<ChallengeResponse> for hv::ChallengeResponse {
    fn from(response: ChallengeResponse) -> Self {
        match response {
            ChallengeResponse::Success { token, ttype } => Self::Success { token, ttype },
            ChallengeResponse::Failure => Self::Failure,
            ChallengeResponse::Cancelled => Self::Cancelled,
        }
    }
}

/// A response to an HTTP request sent by the loader.
#[derive(Debug, uniffi::Record)]
pub struct ChallengeLoaderResponse {
    /// The HTTP status code of the response.
    pub status: u16,

    /// The HTTP status text of the response.
    pub reason: Option<String>,

    /// The content type of the response.
    pub content_type: Option<String>,

    /// The content encoding of the response.
    pub content_encoding: Option<String>,

    /// The headers of the response.
    pub headers: Vec<Header>,

    /// The contents of the response.
    pub contents: Vec<u8>,
}

impl From<hv::ChallengeLoaderResponse> for ChallengeLoaderResponse {
    fn from(res: hv::ChallengeLoaderResponse) -> Self {
        Self {
            status: res.status,
            reason: res.reason,
            content_type: res.content_type,
            content_encoding: res.content_encoding,
            headers: res.headers.into_iter().map_into().collect(),
            contents: res.contents.to_vec(),
        }
    }
}

/// The payload of a human verification challenge.
#[derive(Debug, uniffi::Object)]
pub struct ChallengeLoader {
    inner: hv::ChallengeLoader,
}

/// Create a new `ChallengeLoader`.
#[uniffi_export]
pub async fn new_challenge_loader(
    cfg: ApiConfig,
    app: AppDetails,
) -> Result<Arc<ChallengeLoader>, ProtonError> {
    let cfg = cfg
        .into_real_api_config(app)
        .inspect_err(|e| error!("{e:?}"))
        .map_err(|_| UnexpectedError::Config)?;

    let inner = uniffi_async(async move {
        hv::ChallengeLoader::new(cfg.into())
            .inspect_err(|e| error!("{e:?}"))
            .map_err(|_| RealProtonMailError::Unexpected(Unexpected::Config))
            .await
    })
    .await?;

    Ok(Arc::new(ChallengeLoader { inner }))
}

#[uniffi_export]
impl ChallengeLoader {
    /// Send a GET request to the server and return the response.
    pub async fn get(
        &self,
        base: String,
        path: String,
        query: Vec<Query>,
        header: Vec<Header>,
    ) -> Result<ChallengeLoaderResponse, ProtonError> {
        let inner = self.inner.clone();
        let query = query.into_iter().map_into();
        let header = header.into_iter().map_into();

        uniffi_async(async move {
            inner
                .get(base, path, query, header)
                .map_err(RealUserApiServiceError::try_from)
                .map_err(|e| match e {
                    Ok(e) => RealProtonMailError::ServerError(e.into()),
                    Err(e) => RealProtonMailError::Unexpected(e.into()),
                })
                .await
        })
        .ok_into()
        .err_into()
        .await
    }

    /// Send a POST request to the server and return the response.
    pub async fn post(
        &self,
        base: String,
        path: String,
        query: Vec<Query>,
        header: Vec<Header>,
        body: Vec<u8>,
    ) -> Result<ChallengeLoaderResponse, ProtonError> {
        let inner = self.inner.clone();
        let query = query.into_iter().map_into();
        let header = header.into_iter().map_into();

        uniffi_async(async move {
            inner
                .post(base, path, query, header, body)
                .map_err(RealUserApiServiceError::try_from)
                .map_err(|e| match e {
                    Ok(e) => RealProtonMailError::ServerError(e.into()),
                    Err(e) => RealProtonMailError::Unexpected(e.into()),
                })
                .await
        })
        .ok_into()
        .err_into()
        .await
    }

    /// Send a PUT request to the server and return the response.
    pub async fn put(
        &self,
        base: String,
        path: String,
        query: Vec<Query>,
        header: Vec<Header>,
        body: Vec<u8>,
    ) -> Result<ChallengeLoaderResponse, ProtonError> {
        let inner = self.inner.clone();
        let query = query.into_iter().map_into();
        let header = header.into_iter().map_into();

        uniffi_async(async move {
            inner
                .put(base, path, query, header, body)
                .map_err(RealUserApiServiceError::try_from)
                .map_err(|e| match e {
                    Ok(e) => RealProtonMailError::ServerError(e.into()),
                    Err(e) => RealProtonMailError::Unexpected(e.into()),
                })
                .await
        })
        .ok_into()
        .err_into()
        .await
    }
}

/// An interface by which human verification challenges can be handled.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait ChallengeNotifier: Send + Sync + 'static {
    /// Called when a human verification challenge is encountered.
    ///
    /// The `server` is the server that sent the challenge, and it may be a direct server,
    /// such as `mail.proton.me`, or an indirect server (aka alternative routing),
    /// such as `d{base32}.protonpro.xyz`.
    async fn on_challenge(
        &self,
        server: Arc<ChallengeServer>,
        payload: Arc<ChallengePayload>,
    ) -> ChallengeResponse;
}

#[async_trait::async_trait]
impl<T: ?Sized + ChallengeNotifier> ChallengeNotifier for Arc<T> {
    async fn on_challenge(
        &self,
        server: Arc<ChallengeServer>,
        payload: Arc<ChallengePayload>,
    ) -> ChallengeResponse {
        self.deref().on_challenge(server, payload).await
    }
}

/// Wraps a `ChallengeNotifier` to implement the core `ChallengeNotifier` trait.
pub(crate) struct ChallengeNotifierWrap<T> {
    inner: T,
}

impl<T: ChallengeNotifier> ChallengeNotifierWrap<T> {
    /// Wrap a `ChallengeNotifier` to implement the core `ChallengeNotifier` trait.
    pub fn wrap(inner: T) -> hv::DynChallengeNotifier {
        Arc::new(Self { inner })
    }
}

#[async_trait::async_trait]
impl<T: ChallengeNotifier> hv::ChallengeNotifier for ChallengeNotifierWrap<T> {
    async fn on_challenge(
        &self,
        server: hv::ChallengeServer,
        payload: hv::ChallengePayload,
    ) -> hv::ChallengeResponse {
        let server = ChallengeServer::new(server);
        let payload = ChallengePayload::new(payload);

        self.inner.on_challenge(server, payload).map_into().await
    }
}
