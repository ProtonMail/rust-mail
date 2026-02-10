mod common;

use crate::common::request;
use reqwest::StatusCode;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
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
            .named("Api ping")
            .mount(&mock_server)
            .await;
        // Make a request to the mock server
        let (status, _, _, body) = request(format!("{}/api/ping", mock_server.uri())).await;
        // Assert the response is as expected
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_ref(), b"");
    }
}

#[cfg(test)]
mod messages {
    use super::*;

    use serde_json::json;
    use test_case::test_case;
    use wiremock::matchers::path_regex;

    use proton_core_api::session::Session;
    use proton_core_api::session::{Config, EnvId};
    use proton_core_common::test_utils::test_context::MockApiEnv;
    use proton_core_common::test_utils::utils::mock_auth_endpoints;
    use proton_mail_api::MAX_PAGE_ELEMENT_COUNT_U64;
    use proton_mail_api::services::proton::{ProtonMail, requests::GetMessagesOptions};

    type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;

    #[test_case(MAX_PAGE_ELEMENT_COUNT_U64 - 1, MAX_PAGE_ELEMENT_COUNT_U64 - 1; "Page size smaller than limit")]
    #[test_case(MAX_PAGE_ELEMENT_COUNT_U64, MAX_PAGE_ELEMENT_COUNT_U64; "Page size equal to limit")]
    #[test_case(MAX_PAGE_ELEMENT_COUNT_U64 + 1, MAX_PAGE_ELEMENT_COUNT_U64; "Page size larger than limit")]
    #[tokio::test]
    async fn get_messages_page_size_limit(page_size: u64, want_size: u64) -> Result<()> {
        let server = MockServer::start().await;
        mock_auth_endpoints(&server).await;
        let session = new_session(&server).await?;

        Mock::given(method("GET"))
            .and(path_regex("mail/v4/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Messages": [],
                "Total": 0,
                "Stale": 0,
            })))
            .named("Get messages")
            .mount(&server)
            .await;

        session
            .get_messages(GetMessagesOptions {
                page_size,
                ..Default::default()
            })
            .await?;

        let result = server.received_requests().await.unwrap();
        let last = result.last().unwrap();
        let have_size = last
            .url
            .query_pairs()
            .find_map(|(k, v)| {
                if k == "PageSize" {
                    Some(v.parse::<u64>().unwrap())
                } else {
                    None
                }
            })
            .unwrap();

        assert_eq!(have_size, want_size);

        Ok(())
    }

    /// Create a new session which sends requests to the given mock server.
    async fn new_session(server: &MockServer) -> Result<Session> {
        let config = Config {
            env_id: EnvId::new_custom(MockApiEnv::new(server.uri())),
            ..Default::default()
        };

        Ok(Session::builder().with_config(config).build().await?)
    }
}
