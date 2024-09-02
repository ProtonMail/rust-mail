mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_core_common::datatypes::LabelId;
use proton_mail_common::datatypes::SystemLabelId;

use ctor::ctor;

#[ctor]
fn init_color_backtrace() {
    color_backtrace::install();
}

#[tokio::test]
async fn test_init_after_login() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let init_params = TestParams::default_basic();
    let cb = NullCallback {};
    ctx.setup_user(init_params).await;
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &cb)
        .await
        .expect("failed to initialize");
}

#[tokio::test]
async fn test_double_init_does_not_fail() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;
    let init_params = TestParams::default_basic();
    let cb = NullCallback {};
    ctx.setup_user_repeated(init_params, 2).await;
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &cb)
        .await
        .expect("failed to initialize");
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &cb)
        .await
        .expect("failed to initialize");
}
