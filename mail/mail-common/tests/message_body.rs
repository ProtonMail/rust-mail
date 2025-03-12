use proton_api_core::services::proton::common::{LabelId, UserId};
use proton_mail_common::cache::CacheMessageKey;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::decrypted_message::StorableMessageBody;
use proton_mail_common::models::Message;
use proton_mail_common::{AppError, MailContextError, Mailbox};
use proton_mail_test_utils::message_body::{
    TEST_MESSAGE_BODY_DECRYPTED, TEST_MESSAGE_BODY_MIME_DECRYPTED,
    TEST_MESSAGE_BODY_MIME_SIGNATURE, TEST_USER_ID, message_body_test_message_mime,
    message_body_test_message_simple, message_body_test_params, message_body_test_user_secret,
};
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;

#[tokio::test]
async fn mailbox_message_body_simple() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = message_body_test_params();
    let user_ctx = ctx.mail_user_context().await;

    let message = message_body_test_message_simple();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create a mailbox and sync.
    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
        .await
        .unwrap();
    let mut tether = user_ctx.user_stash().connection();
    // Resolve local id.
    let saved_message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(saved_message.remote_id, Some(message.metadata.id));

    // No message cached
    let cache = user_ctx.messages_cache();
    assert!(cache.is_empty());

    // Decrypt the message body.
    let _local_id = saved_message.local_id.unwrap();

    let decrypted_body = saved_message
        .fetch_message_body(user_ctx.clone(), &mut tether)
        .await
        .unwrap();

    assert_eq!(decrypted_body.body, TEST_MESSAGE_BODY_DECRYPTED);

    // Now a message is cached and it's the right one
    assert_eq!(cache.len(), 1);
    let key = CacheMessageKey::from(&saved_message);
    let item = cache
        .get_item(&key)
        .unwrap()
        .map(|f| StorableMessageBody::from_reader(f).unwrap().body);
    assert_eq!(item, Some(TEST_MESSAGE_BODY_DECRYPTED.to_owned()));

    // Only one call to API is done
    saved_message
        .fetch_message_body(user_ctx.clone(), &mut tether)
        .await
        .unwrap();
    assert_eq!(cache.len(), 1);
}

#[tokio::test]
async fn mailbox_message_body_mime() {
    // Setup:
    //   * Create a message encrypted with MIME format
    //     + Contains 2 attachments
    //     + OpenPGP public key

    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let user_ctx = ctx.mail_user_context().await;

    let message = message_body_test_message_mime();

    let params = message_body_test_params();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut user_ctx.user_stash().connection(), user_ctx.api(), 10)
        .await
        .unwrap();
    let mut tether = user_ctx.user_stash().connection();
    let saved_message = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(saved_message.remote_id, Some(message.metadata.id));

    let cache = user_ctx.messages_cache();
    assert!(cache.is_empty());

    let _local_id = saved_message.local_id.unwrap();

    // Action:
    //   * Get message body and PGP attachments
    let decrypted_message = saved_message
        .fetch_message_body(user_ctx.clone(), &mut tether)
        .await
        .unwrap();

    let Err(MailContextError::App(AppError::UnknownCid(_, cids))) = decrypted_message
        .get_embedded_attachment(&user_ctx, "fail")
        .await
    else {
        panic!("Expected error when passing bad cid");
    };

    for cid in cids {
        decrypted_message
            .get_embedded_attachment(&user_ctx, &cid)
            .await
            .unwrap();
    }
    // Validation:
    assert_eq!(decrypted_message.body, TEST_MESSAGE_BODY_MIME_DECRYPTED);
    let pgp_attachments = decrypted_message.pgp_attachments.unwrap();
    assert_eq!(pgp_attachments.len(), 3);
    assert_eq!(pgp_attachments[0].name, "attachment1.txt");
    assert_eq!(pgp_attachments[0].mime_type, "text/plain");
    assert_eq!(pgp_attachments[0].data, b"attachment1");
    assert_eq!(pgp_attachments[1].name, "attachment2.txt");
    assert_eq!(pgp_attachments[1].mime_type, "text/plain");
    assert_eq!(pgp_attachments[1].data, b"attachment2");
    assert_eq!(pgp_attachments[2].name, "OpenPGP_0x46F0FA708D336220.asc");
    assert_eq!(pgp_attachments[2].mime_type, "application/pgp-keys");
    assert_eq!(
        pgp_attachments[2].data,
        TEST_MESSAGE_BODY_MIME_SIGNATURE.as_bytes()
    );
}
