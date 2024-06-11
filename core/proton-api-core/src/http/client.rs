use crate::http::{Proxy, RequestData, Result};
use std::time::Duration;

use super::APIEnvConfig;

/// Builder for an http client
#[derive(Debug, Clone)]
pub struct Builder {
    pub(super) api_env_config: APIEnvConfig,
    pub(super) request_timeout: Option<Duration>,
    pub(super) connect_timeout: Option<Duration>,
    pub(super) proxy_url: Option<Proxy>,
    pub(super) debug: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            api_env_config: APIEnvConfig::default(),
            proxy_url: None,
            request_timeout: None,
            connect_timeout: None,
            debug: false,
        }
    }

    /// Override the default environment configuration.
    #[must_use]
    pub fn api_env_config(mut self, api_env_config: APIEnvConfig) -> Self {
        self.api_env_config = api_env_config;
        self
    }

    /// Set the full request timeout. By default there is no timeout.
    #[must_use]
    pub fn request_timeout(mut self, duration: Duration) -> Self {
        self.request_timeout = Some(duration);
        self
    }

    /// Set the connection timeout. By default there is no timeout.
    #[must_use]
    pub fn connect_timeout(mut self, duration: Duration) -> Self {
        self.connect_timeout = Some(duration);
        self
    }

    /// Specify proxy URL for the builder.
    #[must_use]
    pub fn with_proxy(mut self, proxy: Proxy) -> Self {
        self.proxy_url = Some(proxy);
        self
    }

    /// Enable request debugging.
    #[must_use]
    pub fn debug(mut self) -> Self {
        self.debug = true;
        self
    }

    /// Build the client.
    ///
    /// # Errors
    /// Returns error if we fail to build the client.
    pub fn build(self) -> std::result::Result<Client, anyhow::Error> {
        Client::try_from(self)
    }
}

#[allow(clippy::module_name_repetitions)]
/// Abstraction over the underlying client request.
pub trait ClientRequest: Sized + Send {
    #[must_use]
    fn header(self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self;

    #[must_use]
    fn bearer_token(self, token: impl AsRef<str>) -> Self {
        self.header("authorization", format!("Bearer {}", token.as_ref()))
    }
}

#[allow(clippy::module_name_repetitions)]
/// Abstraction over the underlying client request.
pub trait ClientRequestBuilder: Clone {
    type Request: ClientRequest;
    fn new_request(&self, data: &RequestData) -> Self::Request;
}

/// HTTP Client abstraction
pub type Client = crate::http::reqwest_client::ReqwestClient;

/// Trait which defines how to retrieve information from an http response.
pub trait FromResponse {
    /// Output of the processing.
    type Output;
    /// Whether the response needs access to the body contents.
    const NEEDS_BODY: bool;
    /// Convert the response body into the desired output.
    ///
    /// # Errors
    /// Returns error on failure.
    fn from_response(response: bytes::Bytes, debug: bool) -> Result<Self::Output>;
}
