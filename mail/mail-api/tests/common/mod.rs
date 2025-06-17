#![allow(unused)]

use bytes::Bytes;
use reqwest::{Client, StatusCode, header::CONTENT_LENGTH, header::CONTENT_TYPE};

/// Send a GET request to the specified URL and return the response.
///
/// This function is used to make requests to external services in tests. It
/// provides a convenient way to make requests and get the response status,
/// body, and other details.
///
/// In most cases this function won't be necessary, as requests are fired from
/// within the library code itself.
pub async fn request(url: String) -> (StatusCode, Option<String>, Option<usize>, Bytes) {
    let response = Client::new().get(url).send().await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .map(ToOwned::to_owned);
    let content_len = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let body = response.bytes().await.unwrap();
    (status, content_type, content_len, body)
}
