use crate::service::ApiServiceError;
use crate::services::proton::BuildError;
use crate::session::{Config, Session};
use bytes::Bytes;
use futures::TryFutureExt;
use muon::ProtonRequest;
use muon::common::Server;
use muon::{Method, ProtonResponse};
use std::str::FromStr;

/// The type of a challenge loader result.
pub type ChallengeLoaderResult<E = ApiServiceError> = Result<ChallengeLoaderResponse, E>;

/// A response to an HTTP request sent by the loader.
#[derive(Debug)]
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
    pub headers: Vec<(String, String)>,

    /// The contents of the response.
    pub contents: Bytes,
}

impl From<ProtonResponse> for ChallengeLoaderResponse {
    fn from(res: ProtonResponse) -> Self {
        let mut headers = Vec::new();
        let mut content_type = None;
        let mut content_encoding = None;

        for (k, v) in res.headers() {
            let k = k.as_str().to_owned();
            let v = v.as_bytes().to_owned();

            if k.eq_ignore_ascii_case("content-type") {
                content_type = String::from_utf8(v.clone()).ok();
            }

            if k.as_str().eq_ignore_ascii_case("content-encoding") {
                content_encoding = String::from_utf8(v.clone()).ok();
            }

            if let Ok(v) = String::from_utf8(v) {
                headers.push((k, v));
            }
        }

        let status = res.status().as_u16();
        let reason = res.status().canonical_reason().map(str::to_owned);
        let contents = res.into_body().into();

        Self {
            status,
            reason,
            content_type,
            content_encoding,
            headers,
            contents,
        }
    }
}

/// The payload of a human verification challenge.
#[derive(Debug, Clone)]
pub struct ChallengeLoader {
    inner: Session,
}

impl ChallengeLoader {
    /// Create a new `ChallengeLoader`.
    pub async fn new(cfg: Config) -> Result<Self, BuildError> {
        Ok(Self {
            inner: Session::builder().with_config(cfg).build().await?,
        })
    }

    /// Make a `GET` request to the given base/path.
    pub async fn get(
        &self,
        base: impl AsRef<str>,
        path: impl AsRef<str>,
        query: impl IntoIterator<Item = (String, Option<String>)>,
        header: impl IntoIterator<Item = (String, String)>,
    ) -> Result<ChallengeLoaderResponse, ApiServiceError> {
        self.send(Method::GET, base, path, query, header, None)
            .ok_into()
            .await
    }

    /// Make a `POST` request to the given base/path.
    pub async fn post(
        &self,
        base: impl AsRef<str>,
        path: impl AsRef<str>,
        query: impl IntoIterator<Item = (String, Option<String>)>,
        header: impl IntoIterator<Item = (String, String)>,
        body: impl Into<Vec<u8>>,
    ) -> Result<ChallengeLoaderResponse, ApiServiceError> {
        self.send(Method::POST, base, path, query, header, Some(body.into()))
            .ok_into()
            .await
    }

    /// Make a `PUT` request to the given base/path.
    pub async fn put(
        &self,
        base: impl AsRef<str>,
        path: impl AsRef<str>,
        query: impl IntoIterator<Item = (String, Option<String>)>,
        header: impl IntoIterator<Item = (String, String)>,
        body: impl Into<Vec<u8>>,
    ) -> Result<ChallengeLoaderResponse, ApiServiceError> {
        self.send(Method::PUT, base, path, query, header, Some(body.into()))
            .ok_into()
            .await
    }

    async fn send(
        &self,
        method: Method,
        base: impl AsRef<str>,
        path: impl AsRef<str>,
        query: impl IntoIterator<Item = (String, Option<String>)>,
        header: impl IntoIterator<Item = (String, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<ProtonResponse, ApiServiceError> {
        let mut req = ProtonRequest::new(method, path);

        for (k, v) in query {
            if let Some(v) = v {
                req = req.query((k, v));
            } else {
                req = req.query((k,));
            }
        }

        for (k, v) in header {
            req = req.header((k, v));
        }

        if let Some(body) = body {
            req = req.body(body);
        }

        req = req.servers([
            Server::from_str(base.as_ref())?,
            Server::from_str(base.as_ref())?.to_indirect(),
        ]);

        Ok(self.inner.send(req).await?)
    }
}
