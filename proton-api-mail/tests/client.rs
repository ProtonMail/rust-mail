mod common;

use crate::common::request;
use proton_async::runtime::LocalRuntime;
use reqwest::StatusCode;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[cfg(test)]
mod basic {
    use super::*;

    #[test]
    fn ping_mock_server() {
        let runtime = LocalRuntime::new().expect("failed to create runtime");
        // Set up mock server with expectations and mock responses
        let mock_server = runtime.block_on(async {
            let mock_server = MockServer::start().await;
            // Simple response check to ensure the setup is working — this endpoint only
            // exists on the mock server
            Mock::given(method("GET"))
                .and(path("/api/ping"))
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&mock_server)
                .await;
            mock_server
        });
        // Make a request to the mock server
        let (status, _, _, body) =
            runtime.block_on(async { request(format!("{}/api/ping", mock_server.uri())).await });
        // Assert the response is as expected
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_ref(), b"");
    }
}
