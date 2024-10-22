use proton_test_utils::test_context::TestContext;

use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn test_mock_context() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    Mock::given(method("GET"))
        .and(path("/api/tests/ping"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(ctx.mock_server())
        .await;

    user_ctx.ping().await.expect("failed to ping");
}
