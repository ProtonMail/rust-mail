use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;

use ctor::ctor;

#[ctor]
fn init_color_backtrace() {
    color_backtrace::install();
}

#[tokio::test]
async fn test_init_after_login() {
    let ctx = MailTestContext::new().await;
    let init_params = TestParams::default_basic();

    ctx.setup_user(init_params).await;
    let _ = ctx.mail_user_context().await;
}

#[tokio::test]
async fn test_double_init_does_not_fail() {
    let ctx = MailTestContext::new().await;
    let init_params = TestParams::default_basic();

    ctx.setup_user_repeated(init_params, 1).await;

    let _user_ctx = ctx.mail_user_context().await;
    let _user_ctx = ctx.mail_user_context().await;
}

#[tokio::test]
async fn test_second_init_works_if_first_fails() {
    // Case where backend had an error, returning 404 but it was fixed afterwards.

    let ctx = MailTestContext::new().await;
    let init_params = TestParams::default_basic();

    let user_ctx_res = ctx.try_mail_user_context().await;

    assert!(user_ctx_res.is_err(), "Expected the first init to fail");

    ctx.setup_user_repeated(init_params, 1).await;

    let _user_ctx = ctx.mail_user_context().await;
}

#[tokio::test]
async fn test_initialized_returns_none_when_no_context() {
    let ctx = MailTestContext::new().await;
    let user_ctx_opt = ctx.initialized_mail_user_context().await;
    assert!(user_ctx_opt.is_none());
}

#[tokio::test]
async fn test_initialized_returns_none_when_context_is_not_initialized() {
    let ctx = MailTestContext::new().await;
    let _ = ctx.uninitialized_mail_user_context().await;
    let user_ctx_opt = ctx.initialized_mail_user_context().await;
    assert!(user_ctx_opt.is_none());
}

#[tokio::test]
async fn test_initialized_returns_some_if_context_is_initialized() {
    let ctx = MailTestContext::new().await;

    let init_params = TestParams::default_basic();
    ctx.setup_user_repeated(init_params, 1).await;

    let old_ctx = ctx.mail_user_context().await;
    tracing::info!("Initialized");

    let user_ctx_opt = ctx.initialized_mail_user_context().await;
    assert!(user_ctx_opt.is_some());

    // In order to have it retained
    drop(old_ctx);
}
