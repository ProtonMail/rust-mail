use crate::service::ApiServiceError;
use crate::services::proton::BuildError;
use crate::session::{Config, Session};
use bytes::Bytes;
use derive_more::Debug;
use futures::TryFutureExt;
use muon::ProtonRequest;
use muon::common::Server;
use muon::{Method, ProtonResponse};
use proton_task_service::SpawnerRef;
use tracing::info;

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
    pub async fn new(cfg: Config, spawner: WeakSpawner) -> Result<Self, BuildError> {
        Ok(Self {
            inner: Session::builder().with_config(cfg).build(spawner).await?,
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
        let base = base.as_ref();
        let path = path.as_ref();
        let query = query.into_iter().collect::<Vec<_>>();
        let header = header.into_iter().collect::<Vec<_>>();

        info!(?base, ?path, ?query);

        self.send(Method::GET, base.parse()?, path, query, header)
            .ok_into()
            .await
    }

    async fn send(
        &self,
        method: Method,
        server: Server,
        path: impl AsRef<str>,
        query: impl IntoIterator<Item = (String, Option<String>)>,
        header: impl IntoIterator<Item = (String, String)>,
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

        Ok(self.inner.send(req.server(server)).await?)
    }
}
