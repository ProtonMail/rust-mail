mod common;

use common::TestContext;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[test]
fn test_mock_context() {
    let ctx = TestContext::new();
    let user_ctx = ctx.user_context();
    ctx.async_runtime().block_on(async {
        Mock::given(method("GET"))
            .and(path("/api/tests/ping"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(ctx.mock_server())
            .await;

        user_ctx.ping().await.expect("failed to ping");
    });
}
