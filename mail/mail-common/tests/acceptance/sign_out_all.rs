use proton_core_common::datatypes::{LocalLabelId, SystemLabel};
use proton_core_common::models::{AppProtection, AppSettings, PinProtection};
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;

const CACHED_FILE_NAME: &str = "my_file.txt";

#[tokio::test]
async fn sign_out_all() {
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

    user_ctx.sign_out_all().await.unwrap();

    for user_ctx in all_user_ctxs.iter() {
        // Make sure we no longer are able to read from user database
        let tether = user_ctx.user_stash().connection().await.unwrap();
        let error = SystemLabel::Inbox
            .local_id(&tether)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("no such table: labels"));

        let error = user_ctx.user().await.unwrap_err().to_string();
        assert!(error.contains("no such table: users"));

        // And account databse should not be dropped.
        // User is no longer logged in, so we should get an error.
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
