use crate::http::{Proxy, RequestData, Result};
use std::time::Duration;

use super::APIEnvConfig;

/// Builder for an http client
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    pub(super) api_env_config: APIEnvConfig,
    pub(super) request_timeout: Option<Duration>,
    pub(super) connect_timeout: Option<Duration>,
    pub(super) proxy_url: Option<Proxy>,
    pub(super) debug: bool,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self {
            api_env_config: APIEnvConfig::default(),
            proxy_url: None,
            request_timeout: None,
            connect_timeout: None,
            debug: false,
        }
    }

    pub fn api_env_config(mut self, api_env_config: APIEnvConfig) -> Self {
        self.api_env_config = api_env_config;
        self
    }

    /// Set the full request timeout. By default there is no timeout.
    pub fn request_timeout(mut self, duration: Duration) -> Self {
        self.request_timeout = Some(duration);
        self
    }

    /// Set the connection timeout. By default there is no timeout.
    pub fn connect_timeout(mut self, duration: Duration) -> Self {
        self.connect_timeout = Some(duration);
        self
    }

    /// Specify proxy URL for the builder.
    pub fn with_proxy(mut self, proxy: Proxy) -> Self {
        self.proxy_url = Some(proxy);
        self
    }

    /// Enable request debugging.
    pub fn debug(mut self) -> Self {
        self.debug = true;
        self
    }

    pub fn build(self) -> std::result::Result<Client, anyhow::Error> {
        Client::try_from(self)
    }
}
pub trait ClientRequest: Sized + Send {
    fn header(self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self;

    fn bearer_token(self, token: impl AsRef<str>) -> Self {
        self.header("authorization", format!("Bearer {}", token.as_ref()))
    }
}

pub trait ClientRequestBuilder: Clone {
    type Request: ClientRequest;
    fn new_request(&self, data: &RequestData) -> Self::Request;
}

/// HTTP Client abstraction
pub type Client = crate::http::reqwest_client::ReqwestClient;

pub trait FromResponse {
    type Output;
    const NEEDS_BODY: bool;
    fn from_response<T: AsRef<[u8]>>(response: T, debug: bool) -> Result<Self::Output>;
}
