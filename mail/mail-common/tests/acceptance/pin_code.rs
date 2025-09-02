use proton_core_common::datatypes::{LocalLabelId, SystemLabel};
use proton_core_common::models::{AppProtection, AppSettings, PinProtection};
use proton_core_common::pin_code::{PinCode, PinError};
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use proton_mail_common::{MailContextError, MailUserContext};
use stash::orm::Model;
use test_case::test_case;

const CACHED_FILE_NAME: &str = "my_file.txt";

async fn test_case_validate_pin_code(user_ctx: &MailUserContext) {
    set_default_pin_code(user_ctx).await;

    // Verify PIN code
    let error = user_ctx
        .mail_context_arc()
        .verify_pin_code(vec![])
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        MailContextError::Pin(PinError::TooManyAttempts)
    ));
}

async fn test_case_delete_pin_code(user_ctx: &MailUserContext) {
    set_default_pin_code(user_ctx).await;

    // Delete PIN code
    let error = user_ctx
        .mail_context_arc()
        .delete_pin_code(vec![])
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        MailContextError::Pin(PinError::TooManyAttempts)
    ));
}

#[test_case(test_case_validate_pin_code)]
#[test_case(test_case_delete_pin_code)]
#[tokio::test]
async fn sign_out_all_on_too_many_attempts_of_pin_code_action(
    case: impl AsyncFnOnce(&MailUserContext),
) {
    // General setup
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params.clone()).await;
    ctx.core_test_context()
        .new_account("OTHER_USER_ID".into(), "OTHER_SESSION_ID".into(), None)
        .await;

    let user_ctx = ctx.mail_user_context().await;
    let all_user_ctxs = user_ctx.all_mail_user_ctxs().await.unwrap();
    assert_eq!(all_user_ctxs.len(), 2);

    for user_ctx in all_user_ctxs.iter() {
        // Make sure we can read from user databases
        let tether = user_ctx.user_stash().connection().await.unwrap();
        let inbox_local_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();

        assert_eq!(inbox_local_id, LocalLabelId::from(1));
        // And account databse
        let _account = user_ctx.account_details().await.unwrap();

        // Add some stuff to the cache
        let mail_ctx = user_ctx.mail_context();
        let mail_cache = mail_ctx.mail_cache_path_for(user_ctx.user_id());
        let contents = "First line.\nSecond line.\nThird line.\n";

        tokio::fs::write(mail_cache.join(CACHED_FILE_NAME), contents.as_bytes())
            .await
            .unwrap();

        assert!(mail_cache.join(CACHED_FILE_NAME).exists());
    }

    case(&user_ctx).await;

    // Test that we perform sign out all
    for user_ctx in all_user_ctxs.iter() {
        // Make sure we no longer are able to read from user database
        let tether = user_ctx.user_stash().connection().await.unwrap();
        let error = SystemLabel::Inbox
            .local_id(&tether)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("no such table: labels"));

        let error = user_ctx.account_details().await.unwrap_err().to_string();
        assert!(error.contains("Account with user id"));
        assert!(error.contains("is missing in the DB"));

        // And that cache is empty
        let mail_ctx = user_ctx.mail_context();
        let mail_cache = mail_ctx.mail_cache_path_for(user_ctx.user_id());

        assert!(!mail_cache.join(CACHED_FILE_NAME).exists());
        assert!(!mail_cache.exists());

        let core_user_ctx = user_ctx.user_context();
        let core_cache = core_user_ctx.cache_path();
        let user_db_path = core_user_ctx.get_user_db_path();

        assert!(core_cache.exists());
        assert!(!user_db_path.exists());
    }

    // Check that app settings and pin protection are reset
    let tether = user_ctx
        .core_context()
        .account_stash()
        .connection()
        .await
        .unwrap();
    let app_settings = AppSettings::get_or_default(&tether).await;
    assert_eq!(app_settings.protection, AppProtection::None);

    let pin_metadata = PinProtection::get(&tether).await.unwrap();
    assert!(pin_metadata.is_none());
}

async fn set_default_pin_code(user_ctx: &MailUserContext) {
    let mut tether = user_ctx
        .core_context()
        .account_stash()
        .connection()
        .await
        .unwrap();

    // Set PIN code
    PinCode::set(user_ctx.core_context().clone(), vec![1, 2, 3, 4])
        .await
        .unwrap();

    // Make this attempt a last one
    let mut pin_metadata = PinProtection::get(&tether).await.unwrap().unwrap();
    pin_metadata.attempts = PinCode::MAX_ATTEMPTS - 1;
    user_ctx.core_context().clock().pin_code_reset();

    tether
        .tx(async |bond| pin_metadata.save(bond).await)
        .await
        .unwrap();
}
