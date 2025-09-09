//! Definitions for common HTTP headers.

use crate::http::AsHeader;

/// A content type header.
#[derive(Debug)]
pub struct ContentType<T: AsRef<str>>(pub T);

impl ContentType<&'static str> {
    /// The `application/json` content type.
    pub const JSON: Self = Self("application/json");

    /// The `application/x-www-form-urlencoded` content type.
    pub const FORM: Self = Self("application/x-www-form-urlencoded");

    /// The `application/dns-message` content type.
    pub const DNS: Self = Self("application/dns-message");
}

impl<T: AsRef<str>> AsHeader for ContentType<T> {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        [("content-type".into(), self.0.as_ref().to_owned())]
    }
}

/// An accept header.
#[derive(Debug)]
pub struct Accept<T: AsRef<str>>(pub T);

impl Accept<&'static str> {
    /// The `application/json` accept header.
    pub const JSON: Self = Self("application/json");

    /// The `application/dns-message` accept header.
    pub const DNS: Self = Self("application/dns-message");
}

impl<T: AsRef<str>> AsHeader for Accept<T> {
    fn as_header(&self) -> impl IntoIterator<Item = (String, String)> {
        [("accept".into(), self.0.as_ref().to_owned())]
    }
}
