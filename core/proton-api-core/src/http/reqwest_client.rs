use super::APIEnvConfig;
use crate::http::{
    ClientBuilder, ClientRequest, ClientRequestBuilder, FromResponse, HttpRequestError, Method,
    Request, RequestData, X_PM_APP_VERSION_HEADER,
};
use crate::requests::APIError;
use reqwest;

#[derive(Debug, Clone)]
pub struct ReqwestClient {
    pub(crate) api_env_config: APIEnvConfig,
    client: reqwest::Client,
    debug: bool,
}

impl TryFrom<ClientBuilder> for ReqwestClient {
    type Error = anyhow::Error;

    fn try_from(value: ClientBuilder) -> Result<Self, Self::Error> {
        let mut header_map = reqwest::header::HeaderMap::new();
        header_map.insert(
            X_PM_APP_VERSION_HEADER,
            reqwest::header::HeaderValue::from_str(&value.api_env_config.app_version)
                .map_err(|e| anyhow::anyhow!(e))?,
        );

        let mut builder = reqwest::ClientBuilder::new();

        builder = builder.default_headers(header_map);

        #[cfg(not(feature = "web"))]
        {
            use reqwest::tls::Version;

            if let Some(proxy) = value.proxy_url {
                let proxy = reqwest::Proxy::all(proxy.as_url())?;
                builder = builder.proxy(proxy);
            }

            if let Some(d) = value.connect_timeout {
                builder = builder.connect_timeout(d)
            }

            if let Some(d) = value.request_timeout {
                builder = builder.timeout(d)
            }

            builder = builder
                .min_tls_version(Version::TLS_1_2)
                .cookie_store(true)
                .user_agent(&value.api_env_config.user_agent)
                .https_only(!value.api_env_config.allow_http);
        }

        Ok(Self {
            api_env_config: value.api_env_config,
            client: builder.build()?,
            debug: value.debug,
        })
    }
}

impl From<reqwest::Error> for HttpRequestError {
    fn from(value: reqwest::Error) -> Self {
        // Check timeout before all other errors as it can be produced by multiple
        // reqwest error kinds.
        if value.is_timeout() {
            return HttpRequestError::Timeout(anyhow::Error::new(value));
        }

        #[cfg(not(feature = "web"))]
        if value.is_connect() {
            return HttpRequestError::Connection(anyhow::Error::new(value));
        }

        if value.is_body() {
            HttpRequestError::Request(anyhow::Error::new(value))
        } else if value.is_redirect() {
            HttpRequestError::Redirect(
                value
                    .url()
                    .map(|v| v.to_string())
                    .unwrap_or("Unknown URL".to_string()),
                anyhow::Error::new(value),
            )
        } else if value.is_request() {
            HttpRequestError::Request(anyhow::Error::new(value))
        } else {
            HttpRequestError::Other(anyhow::Error::new(value))
        }
    }
}

pub struct ReqwestRequest(reqwest::RequestBuilder);

impl ClientRequest for ReqwestRequest {
    fn header(self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        Self(self.0.header(key.as_ref(), value.as_ref()))
    }
}

impl ClientRequestBuilder for ReqwestClient {
    type Request = ReqwestRequest;

    fn new_request(&self, data: &RequestData) -> Self::Request {
        let final_url = format!("{}/{}", self.api_env_config.base_url, data.url);

        let mut request = match data.method {
            Method::Delete => self.client.delete(&final_url),
            Method::Get => self.client.get(&final_url),
            Method::Put => self.client.put(&final_url),
            Method::Post => self.client.post(&final_url),
            Method::Patch => self.client.patch(&final_url),
        };

        // Set headers.
        for (header, value) in &data.headers {
            request = request.header(header, value);
        }

        if !data.queries.is_empty() {
            request = request.query(&data.queries);
        }

        if let Some(body) = &data.body {
            request = request.body(body.clone())
        }

        ReqwestRequest(request)
    }
}

impl ReqwestClient {
    pub async fn direct_exec<R: FromResponse>(
        &self,
        r: ReqwestRequest,
    ) -> crate::http::Result<R::Output> {
        let response = r.0.send().await?;

        let status = response.status().as_u16();

        if status >= 400 {
            let body = response
                .bytes()
                .await
                .map_err(|_| HttpRequestError::API(APIError::new(status)))?;

            return Err(HttpRequestError::API(APIError::with_status_and_body(
                status,
                body.as_ref(),
            )));
        }

        if !R::NEEDS_BODY {
            return R::from_response([], self.debug);
        }

        let bytes = response.bytes().await?;

        R::from_response(bytes, self.debug)
    }
}

impl ReqwestClient {
    pub async fn execute_request<R: Request>(
        &self,
        request: R,
    ) -> crate::http::Result<<R::Response as FromResponse>::Output> {
        let r = request.build(self);
        self.direct_exec::<R::Response>(r).await
    }
    pub async fn execute<R: FromResponse>(
        &self,
        request: ReqwestRequest,
    ) -> crate::http::Result<R::Output> {
        self.direct_exec::<R>(request).await
    }
}
