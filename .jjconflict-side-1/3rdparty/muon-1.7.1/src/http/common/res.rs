//! ## HTTP Response
//!
//! This module implements an HTTP response type.
//! It provides methods for accessing and working with the status code, headers,
//! and body of the response.
//!
//! Internally, it's a wrapper around a [`http::Response<Vec<u8>>`], but this is
//! not exposed in the public API, enabling us to change the internal
//! representation if needed.

use crate::common::{Name, Server};
use crate::{ErrorKind, Result};
use derive_more::{Debug, Display};
use serde::Deserialize;
use thiserror::Error;

/// Represents an HTTP status code.
pub type Status = http::StatusCode;

/// Represents the headers of an HTTP response.
pub type Headers = http::HeaderMap;

/// An error indicating that a response has a 4xx or 5xx status code.
/// The status code and response are included in the error.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct StatusErr(pub Status, pub Box<HttpRes>);

impl From<StatusErr> for crate::Error {
    fn from(err: StatusErr) -> Self {
        ErrorKind::res(err)
    }
}

/// An HTTP response.
#[derive(Debug, Display)]
#[display("'{}'", res.status())]
pub struct HttpRes<B = Vec<u8>> {
    server: Server,
    name: Name,
    res: http::Response<B>,
}

impl<B> HttpRes<B> {
    /// Create a new HTTP response.
    pub(crate) fn new(server: Server, name: Name, res: http::Response<B>) -> Self {
        Self { server, name, res }
    }
}

impl HttpRes {
    /// Get the server that sent the response.
    pub fn server(&self) -> &Server {
        &self.server
    }

    /// Get the resolved name of the server that sent the response.
    #[must_use]
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// Get the status code of the response.
    #[must_use]
    pub fn status(&self) -> Status {
        self.res.status()
    }

    /// Return whether the status code is equal to the given value.
    #[must_use]
    pub fn is(&self, status: impl TryInto<Status>) -> bool {
        if let Ok(status) = status.try_into() {
            self.status() == status
        } else {
            false
        }
    }

    /// Ensure the response has a non-error status code.
    ///
    /// # Errors
    ///
    /// Returns an error if the status code is not 1xx, 2xx or 3xx.
    pub fn ok(self) -> Result<Self, StatusErr> {
        match self.status() {
            s if s.is_informational() => Ok(self),
            s if s.is_success() => Ok(self),
            s if s.is_redirection() => Ok(self),

            s => Err(StatusErr(s, self.into()))?,
        }
    }

    /// Get an iterator over the headers of the response.
    ///
    /// The headers are returned as tuples of byte slices.
    #[must_use]
    pub fn headers(&self) -> &Headers {
        self.res.headers()
    }

    /// Get a reference to the body of the response.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        self.res.body()
    }

    /// Consume the response and return the body.
    #[must_use]
    pub fn into_body(self) -> Vec<u8> {
        self.res.into_body()
    }

    /// Get the body as a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the body is not valid UTF-8.
    pub fn body_str(&self) -> Result<&str> {
        std::str::from_utf8(self.body()).map_err(ErrorKind::res)
    }

    /// Consume the response and return the body as a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the body is not valid UTF-8.
    pub fn into_body_string(self) -> Result<String> {
        String::from_utf8(self.into_body()).map_err(ErrorKind::res)
    }

    /// Deserialize a json body as a type.
    ///
    /// # Errors
    ///
    /// Returns an error if the body cannot be deserialized.
    pub fn body_json<T>(&self) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_slice(self.body()).map_err(ErrorKind::res)
    }

    /// Consume the response and deserialize a json body as a type.
    ///
    /// # Errors
    ///
    /// Returns an error if the body cannot be deserialized.
    pub fn into_body_json<T>(self) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        serde_json::from_slice(&self.into_body()).map_err(ErrorKind::res)
    }
}
