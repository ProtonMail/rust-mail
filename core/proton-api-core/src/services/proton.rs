//! The Proton API service.

use crate::service::{ApiService, ApiServiceError};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, RequestBuilder, Url,
};
use serde::de::DeserializeOwned;

/// A service for communicating with the Proton API.
#[derive(Clone, Debug)]
pub struct Proton {
    /// The Reqwest HTTP client which is used internally.
    client: Client,

    /// The base URL for the external service.
    base_url: Url,

    /// A collection of headers to send with every request.
    headers: HeaderMap,
}

impl ApiService for Proton {
    fn new(base_url: Url, headers: Option<HeaderMap>) -> Self {
        Self {
            client: Client::new(),
            base_url,
            headers: headers.unwrap_or_default(),
        }
    }

    fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn client(&self) -> &Client {
        &self.client
    }

    fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    async fn on_error<T>(
        error: ApiServiceError,
        _request: impl Fn() -> RequestBuilder + Send,
    ) -> Result<T, ApiServiceError>
    where
        T: DeserializeOwned,
    {
        Err(error)
    }

    fn set_header(&mut self, name: &str, value: &str) {
        self.headers.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_bytes(value.as_bytes()).unwrap(),
        );
    }
}
