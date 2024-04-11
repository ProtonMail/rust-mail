mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::LabelId;

#[test]
fn test_init_after_login() {
    let ctx = TestContext::new();
    let user_ctx = ctx.user_context();
    let init_params = TestParams::default_basic();
    let cb = NullCallback {};
    ctx.async_runtime().block_on(async {
        ctx.setup_user(init_params).await;
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");
    });
}
