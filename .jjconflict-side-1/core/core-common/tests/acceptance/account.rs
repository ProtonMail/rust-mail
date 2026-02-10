use proton_core_common::db::account::{CoreAccount, CoreSession};
use proton_core_common::models::ModelExtension;
use proton_core_common::test_utils::test_context::TestContext;

#[tokio::test]
async fn logout_and_delete_user_data_preserves_account_metadata() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;

    let user_id = user_ctx.user_id().clone();

    drop(user_ctx);
    real_ctx
        .logout_and_delete_user_data(user_id.clone(), vec![])
        .await
        .unwrap();

    let tether = real_ctx.account_stash().connection().await.unwrap();
    // No sessions exist
    assert!(CoreSession::all(&tether).await.unwrap().is_empty());

    // Account is still present.
    let _ = CoreAccount::find_by_id(user_id.clone(), &tether)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn delete_account_does_not_preserve_account_metadata() {
    let ctx = TestContext::new().await;
    let real_ctx = ctx.context();
    let user_ctx = ctx.user_context().await;

    let user_id = user_ctx.user_id().clone();

    drop(user_ctx);

    assert!(real_ctx.user_db_path(&user_id).exists());

    real_ctx
        .delete_account(user_id.clone(), vec![])
        .await
        .unwrap();

    assert!(!real_ctx.user_db_path(&user_id).exists());

    let tether = real_ctx.account_stash().connection().await.unwrap();
    // No sessions exist
    assert!(CoreSession::all(&tether).await.unwrap().is_empty());

    // Account is not present anymore.
    assert!(
        CoreAccount::find_by_id(user_id.clone(), &tether)
            .await
            .unwrap()
            .is_none()
    );
}
