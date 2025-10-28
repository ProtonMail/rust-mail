use proton_mail_api::services::proton::response_data::IncomingDefault as ApiIncomingDefault;
use proton_mail_api::services::proton::response_data::IncomingDefaultLocation as ApiIncomingDefaultLocation;
use proton_mail_common::models::{IncomingDefault, IncomingDefaultLocation};
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::stash::StashError;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

fn default_api_incoming_default(email: &str) -> ApiIncomingDefault {
    ApiIncomingDefault {
        email: Some(email.into()),
        location: ApiIncomingDefaultLocation::Blocked,
        action: None,
        id: format!("remote-id-{email}"),
        domain: None,
    }
}

fn domain_api_incoming_default(domain: &str) -> ApiIncomingDefault {
    ApiIncomingDefault {
        email: None,
        location: ApiIncomingDefaultLocation::Blocked,
        action: None,
        id: format!("remote-id-{domain}"),
        domain: Some(domain.into()),
    }
}

async fn count_incoming_defaults_for_email(
    tether: &stash::stash::Tether,
    email: &str,
) -> Result<usize, StashError> {
    let results = IncomingDefault::find(
        "WHERE email = ? AND deleted = 0",
        stash::params![email.to_string()],
        tether,
    )
    .await?;
    Ok(results.len())
}

async fn count_incoming_defaults_for_domain(
    tether: &stash::stash::Tether,
    domain: &str,
) -> Result<usize, StashError> {
    let results = IncomingDefault::find(
        "WHERE domain = ? AND deleted = 0",
        stash::params![domain.to_string()],
        tether,
    )
    .await?;
    Ok(results.len())
}

#[tokio::test]
async fn test_basic_block_sender() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "block_me@example.com";

    test_ctx
        .mock_post_incoming_default_n(default_api_incoming_default(email), 1)
        .await;
    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );

    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(executed_count, 1);

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let incoming_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        incoming_default.email.as_ref().unwrap().as_clear_text_str(),
        email
    );
    assert_eq!(incoming_default.location, IncomingDefaultLocation::Blocked);
    assert!(!incoming_default.deleted);
    assert!(incoming_default.remote_id.is_some());
}

#[tokio::test]
async fn test_basic_unblock_sender() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "unblock_me@example.com";

    test_ctx.mock_delete_incoming_default().await;
    test_ctx.catch_all().await;
    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut incoming_default = IncomingDefault {
                email: Some(email.into()),
                location: IncomingDefaultLocation::Blocked,
                remote_id: Some("existing-remote-id".into()),
                local_id: None,
                domain: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    IncomingDefault::action_unblock(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(executed_count, 1);
    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn test_double_block_idempotent() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "double_block@example.com";

    test_ctx
        .mock_post_incoming_default_n(default_api_incoming_default(email), 1)
        .await;
    test_ctx.catch_all().await;
    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(executed_count, 1);
    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );
    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let _executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn test_double_unblock_idempotent() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "double_unblock@example.com";

    test_ctx.mock_delete_incoming_default().await;
    test_ctx.catch_all().await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut incoming_default = IncomingDefault {
                email: Some(email.into()),
                location: IncomingDefaultLocation::Blocked,
                remote_id: Some("existing-remote-id".into()),
                local_id: None,
                domain: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1 // Sanity check
    );
    IncomingDefault::action_unblock(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );

    IncomingDefault::action_unblock(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn test_block_unblock_cycle() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "cycle_test@example.com";

    test_ctx
        .mock_post_incoming_default_n(default_api_incoming_default(email), 2)
        .await;
    test_ctx.mock_delete_incoming_default().await;
    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );
    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();
    user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );
    IncomingDefault::action_unblock(user_ctx.action_queue(), email.into())
        .await
        .unwrap();
    user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );
    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();
    user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let incoming_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(incoming_default.location, IncomingDefaultLocation::Blocked);
    assert!(!incoming_default.deleted);
}

#[tokio::test]
async fn test_remote_block_failure_with_local_rollback() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "fail_block@example.com";

    Mock::given(method("POST"))
        .and(path("/api/mail/v4/incomingdefaults"))
        .respond_with(ResponseTemplate::new(451).set_body_json(serde_json::json!({
            "error": "Internal server error"
        })))
        .expect(1)
        .mount(test_ctx.mock_server())
        .await;
    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );

    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let _result = user_ctx.execute_all_actions().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn test_remote_unblock_failure_with_proper_error_handling() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "fail_unblock@example.com";

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut incoming_default = IncomingDefault {
                email: Some(email.into()),
                location: IncomingDefaultLocation::Blocked,
                remote_id: Some("existing-remote-id".into()),
                local_id: None,
                domain: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    Mock::given(method("PUT"))
        .and(path("/api/mail/v4/incomingdefaults/delete"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": "Not found"
        })))
        .expect(1)
        .mount(test_ctx.mock_server())
        .await;
    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    IncomingDefault::action_unblock(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let _result = user_ctx.execute_all_actions().await;
}

#[tokio::test]
async fn test_block_sender_when_inbox_location_exists() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "inbox_to_block@example.com";

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut incoming_default = IncomingDefault {
                email: Some(email.into()),
                location: IncomingDefaultLocation::Inbox,
                remote_id: Some(format!("remote-id-{email}").into()),
                local_id: None,
                domain: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    test_ctx
        .mock_put_incoming_default(default_api_incoming_default(email))
        .await;
    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let initial_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(initial_default.location, IncomingDefaultLocation::Inbox);

    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(executed_count, 1);

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let updated_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_default.location, IncomingDefaultLocation::Blocked);
}

#[tokio::test]
async fn test_block_sender_when_spam_location_exists() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "spam_to_block@example.com";

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut incoming_default = IncomingDefault {
                email: Some(email.into()),
                location: IncomingDefaultLocation::Spam,
                remote_id: Some(format!("remote-id-{email}").into()),
                local_id: None,
                domain: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    test_ctx
        .mock_put_incoming_default(default_api_incoming_default(email))
        .await;
    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let initial_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(initial_default.location, IncomingDefaultLocation::Spam);

    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(executed_count, 1);

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let updated_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_default.location, IncomingDefaultLocation::Blocked);
}

#[tokio::test]
async fn test_unblock_sender_when_inbox_location_exists_should_not_work() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "inbox_unblock_attempt@example.com";

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut incoming_default = IncomingDefault {
                email: Some(email.into()),
                location: IncomingDefaultLocation::Inbox,
                remote_id: Some(format!("remote-id-{email}").into()),
                local_id: None,
                domain: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let initial_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(initial_default.location, IncomingDefaultLocation::Inbox);

    IncomingDefault::action_unblock(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(executed_count, 1);

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let unchanged_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged_default.location, IncomingDefaultLocation::Inbox);
    assert!(!unchanged_default.deleted);
}

#[tokio::test]
async fn test_unblock_sender_when_spam_location_exists_should_not_work() {
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "spam_unblock_attempt@example.com";

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut incoming_default = IncomingDefault {
                email: Some(email.into()),
                location: IncomingDefaultLocation::Spam,
                remote_id: Some("existing-spam-id".into()),
                local_id: None,
                domain: None,
                deleted: false,
            };
            incoming_default.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let initial_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(initial_default.location, IncomingDefaultLocation::Spam);

    IncomingDefault::action_unblock(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(executed_count, 1);

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        1
    );

    let unchanged_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged_default.location, IncomingDefaultLocation::Spam);
    assert!(!unchanged_default.deleted);
}

#[tokio::test]
async fn test_api_returning_domain() {
    /*
     * Since our local codebase does not handle domains (yet) lets assume
     * broken scenario where for some reason backend returns an incoming default with a
     * domain instead of an email. In the end we do not control BE...
     */
    let test_ctx = MailTestContext::new().await;
    let user_ctx = test_ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let email = "block_me@example.com";
    let domain = "example.com";

    test_ctx
        .mock_post_incoming_default_n(domain_api_incoming_default(domain), 1)
        .await;
    test_ctx.catch_all().await;

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );

    IncomingDefault::action_block(user_ctx.action_queue(), email.into())
        .await
        .unwrap();

    let executed_count = user_ctx.execute_all_actions().await.unwrap();
    assert_eq!(executed_count, 1);

    assert_eq!(
        count_incoming_defaults_for_email(&tether, email)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        count_incoming_defaults_for_domain(&tether, domain)
            .await
            .unwrap(),
        1
    );

    let incoming_default = IncomingDefault::by_email(email, &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(incoming_default.email.is_none());
    assert_eq!(incoming_default.domain.as_ref().unwrap(), domain);
    assert_eq!(incoming_default.location, IncomingDefaultLocation::Blocked);
    assert!(!incoming_default.deleted);
    // We did not save the response from the Backend (Because we do not support lack of email)
    // but at least we did not panic
    assert!(incoming_default.remote_id.is_some());
}
