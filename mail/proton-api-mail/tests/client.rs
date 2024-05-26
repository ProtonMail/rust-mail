mod common;

use crate::common::request;
use reqwest::StatusCode;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[cfg(test)]
mod basic {
    use super::*;

    #[tokio::test]
    async fn ping_mock_server() {
        // Set up mock server with expectations and mock responses
        let mock_server = MockServer::start().await;
        // Simple response check to ensure the setup is working — this endpoint only
        // exists on the mock server
        Mock::given(method("GET"))
            .and(path("/api/ping"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;
        // Make a request to the mock server
        let (status, _, _, body) = request(format!("{}/api/ping", mock_server.uri())).await;
        // Assert the response is as expected
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_ref(), b"");
    }
}
