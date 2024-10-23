use proton_test_utils::test_context::TestContext;

#[tokio::test]
#[allow(unused_variables)]
async fn test_session_state() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;
}

#[tokio::test]
#[allow(unused_variables)]
async fn test_session_state_watcher() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;
}
