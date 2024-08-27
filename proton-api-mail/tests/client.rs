mod common;

use crate::common::request;
use reqwest::StatusCode;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;

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

#[cfg(test)]
mod messages {
    use std::{error::Error, future::Future};

    use super::*;
    use proton_api_core::{
        services::proton::Config,
        session::{CoreSession, Session},
    };
    use proton_api_mail::{
        services::proton::{requests::GetMessagesOptions, ProtonMail},
        MAX_PAGE_ELEMENT_COUNT_U64,
    };
    use serde_json::{json, Value};
    use wiremock::matchers::path_regex;

    #[tokio::test]
    async fn get_messages_page_size_limit() -> Result<()> {
        let server = MockServer::start().await;
        let session = new_session(&server).await?;

        // Set up mock server with expectations and mock responses
        Mock::given(method("POST"))
            .and(path_regex("mail/v4/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "Messages": [],
                "Total": 0,
                "Stale": 0,
            })))
            .expect(3)
            .mount(&server)
            .await;

        let sizes = [
            MAX_PAGE_ELEMENT_COUNT_U64 - 1,
            MAX_PAGE_ELEMENT_COUNT_U64,
            MAX_PAGE_ELEMENT_COUNT_U64 + 1,
        ];

        for size in sizes {
            let body = get_last_body(&server, || async {
                let mut options = GetMessagesOptions::default();

                options.page_size = size;

                session.api().get_messages(options).await
            })
            .await?;

            assert!(body["PageSize"].as_u64().unwrap() <= MAX_PAGE_ELEMENT_COUNT_U64);
        }

        Ok(())
    }

    /// Create a new session which sends requests to the given mock server.
    async fn new_session(server: &MockServer) -> Result<Session> {
        let config = Config {
            base_url: server.uri().parse()?,

            ..Default::default()
        };

        Ok(Session::new(config, None).await?)
    }

    /// Perform some operation within a callback and return the corresponding request that was received.
    async fn get_last_body<F, T, E>(s: &MockServer, f: impl FnOnce() -> F) -> Result<Value>
    where
        F: Future<Output = Result<T, E>>,
        E: Error + Send + Sync + 'static,
    {
        let _ = f().await?;

        let r = s.received_requests().await.ok_or("no requests received")?;

        let r = r.last().ok_or("request list is empty")?;

        Ok(r.body_json()?)
    }
}
