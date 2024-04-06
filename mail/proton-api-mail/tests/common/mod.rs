use bytes::Bytes;
use reqwest::{header::CONTENT_LENGTH, header::CONTENT_TYPE, Client, StatusCode};

pub async fn request(url: String) -> (StatusCode, Option<String>, Option<usize>, Bytes) {
    let response = Client::new().get(url).send().await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_owned());
    let content_len = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    let body = response.bytes().await.unwrap();
    (status, content_type, content_len, body)
}
