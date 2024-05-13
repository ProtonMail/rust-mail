mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::LabelId;

#[tokio::test]
async fn test_init_after_login() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context();
    let init_params = TestParams::default_basic();
    let cb = NullCallback {};
    ctx.setup_user(init_params).await;
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &cb)
        .await
        .expect("failed to initialize");
}
