#[allow(unused)]
#[path = "../../proton-mail-common/tests/common/mod.rs"]
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
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/ping"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;
        let (status, _, _, body) = request(format!("{}/api/ping", mock_server.uri())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_ref(), b"");
    }
}
