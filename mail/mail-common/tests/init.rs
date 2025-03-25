use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::test_context::MailTestContext;

use ctor::ctor;

#[ctor]
fn init_color_backtrace() {
    color_backtrace::install();
}

#[tokio::test]
async fn test_init_after_login() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let init_params = TestParams::default_basic();

    ctx.setup_user(init_params).await;
    ctx.init_user(user_ctx).await;
}

#[tokio::test]
async fn test_double_init_does_not_fail() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let init_params = TestParams::default_basic();

    ctx.setup_user_repeated(init_params, 1).await;
    ctx.init_user(user_ctx.clone()).await;
    ctx.init_user(user_ctx.clone()).await;
}
