mod common;

use proton_api_mail::domain::LabelId;

#[test]
fn test_init_after_login() {
    let ctx = common::TestContext::new();
    let user_ctx = ctx.user_context();
    ctx.async_runtime().block_on(async {
        let init_params = common::init::Params::default_basic();
        common::init::setup_user(&ctx, init_params).await;
        let cb = common::init::NullCallback {};
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");
    });
}
