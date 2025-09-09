//! wiremock-multipart adds matchers for use with [wiremock](https://crates.io/crates/wiremock)
//! to check multipart characteristics of requests.
//!
//! ## How to install
//! Add `wiremock-multipart` to your dev-dependencies:
//! ```toml
//! [dev-dependencies]
//! # ...
//! wiremock-multipart = "0.1"
//! ```
//!
//! ## Getting started
//!
//! ```rust
//! use wiremock::{MockServer, Mock, ResponseTemplate};
//! use wiremock::matchers::method;
//! use wiremock_multipart::prelude::*;
//!
//! #[async_std::main]
//! async fn main() {
//!     // Start a background HTTP server on a random local port
//!     let mock_server = MockServer::start().await;
//!
//!     // Arrange the behaviour of the MockServer adding a Mock
//!     Mock::given(method("POST"))
//!         .and(NumberOfParts(2))
//!         .respond_with(ResponseTemplate::new(200))
//!         // Mounting the mock on the mock server - it's now effective!
//!         .mount(&mock_server)
//!         .await;
//!
//!     // if we now send a multipart/form-data request with two parts to it, the request
//!     // will match and return 200.
//! }
//! ```

#[cfg(test)]
extern crate indoc;
extern crate lazy_regex;
#[cfg(test)]
extern crate maplit;
extern crate wiremock;

pub mod matchers;
mod part;
mod request_utils;

pub use part::Part;
pub use request_utils::{MultipartContentType, RequestUtils};

pub mod prelude {
    pub use crate::matchers::*;
}

#[cfg(test)]
mod test_utils {
    use maplit::hashmap;
    use std::collections::HashMap;
    use std::str::FromStr;

    use wiremock::http::{HeaderName, HeaderValue, Method, Url};
    use wiremock::Request;

    pub fn name(name: &'static str) -> HeaderName {
        HeaderName::from_str(name).unwrap()
    }

    pub fn values(val: &'static str) -> HeaderValue {
        HeaderValue::from_str(val).unwrap()
    }

    pub fn request(headers: impl IntoIterator<Item = (HeaderName, HeaderValue)>) -> Request {
        requestb(headers, vec![])
    }

    pub fn requestb(
        headers: impl IntoIterator<Item = (HeaderName, HeaderValue)>,
        body: Vec<u8>,
    ) -> Request {
        Request {
            url: Url::from_str("http://localhost").unwrap(),
            method: Method::POST,
            headers: headers.into_iter().collect(),
            body,
        }
    }

    pub fn multipart_header() -> HashMap<HeaderName, HeaderValue> {
        hashmap! {
            name("content-type") => values("multipart/form-data; boundary=xyz"),
        }
    }
}
