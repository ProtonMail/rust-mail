use proton_mail_common::test_utils::test_context::MailTestContext;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn test_mock_context() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    Mock::given(method("GET"))
        .and(path("/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .named("Mock ping")
        .mount(ctx.mock_server())
        .await;

    user_ctx.ping().await.expect("failed to ping");
}
