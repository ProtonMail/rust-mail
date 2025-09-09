use crate::app::{AppVersion, UserAgent};
use crate::http::AsHeader;
use std::borrow::Borrow;

/// The user agent header.
#[derive(Debug)]
pub struct UserAgentHeader(String);

impl UserAgentHeader {
    /// Create a new user agent header.
    pub fn new(value: impl Borrow<UserAgent>) -> Self {
        Self(value.borrow().to_string())
    }
}

impl AsHeader for UserAgentHeader {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        [("user-agent".to_owned(), self.0.clone())]
    }
}

/// The `x-pm-appversion` header.
#[derive(Debug)]
pub struct AppVersionHeader(String);

impl AppVersionHeader {
    /// Create a new app version header.
    pub fn new(value: impl Borrow<AppVersion>) -> Self {
        Self(value.borrow().to_string())
    }
}

impl AsHeader for AppVersionHeader {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        [("x-pm-appversion".to_owned(), self.0.clone())]
    }
}

/// The `x-pm-uid` header.
#[derive(Debug)]
pub struct AuthUidHeader(String);

impl AuthUidHeader {
    /// Create a new auth UID header.
    pub fn new(value: impl AsRef<str>) -> Self {
        Self(value.as_ref().to_owned())
    }
}

impl AsHeader for AuthUidHeader {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        [("x-pm-uid".to_owned(), self.0.clone())]
    }
}

/// The `authorization` header.
#[derive(Debug)]
pub struct AuthTokenHeader(String);

impl AuthTokenHeader {
    /// Create a new auth token header.
    pub fn new(value: impl AsRef<str>) -> Self {
        Self(value.as_ref().to_owned())
    }
}

impl AsHeader for AuthTokenHeader {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        [("authorization".to_owned(), bearer(&self.0))]
    }
}

fn bearer(tok: &str) -> String {
    format!("Bearer {tok}")
}

/// The `x-pm-doh-host` header.
#[derive(Debug)]
pub struct DohHostHeader(String);

impl DohHostHeader {
    /// Create a new DOH host header.
    pub fn new(value: impl AsRef<str>) -> Self {
        Self(value.as_ref().to_owned())
    }
}

impl AsHeader for DohHostHeader {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        [("x-pm-doh-host".to_owned(), self.0.clone())]
    }
}
