use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};
mod common;

#[test]
fn test_mock_context() {
    let ctx = common::TestContext::new();
    let user_ctx = ctx.user_context();
    ctx.async_runtime().block_on(async {
        Mock::given(method("GET"))
            .and(path("/api/tests/ping"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&ctx.mock_server())
            .await;

        user_ctx.ping().await.expect("failed to ping");
    });
}
