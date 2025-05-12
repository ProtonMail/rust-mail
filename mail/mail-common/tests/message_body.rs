use proton_core_api::services::proton::{LabelId, UserId};
use std::str::FromStr;

use indoc::formatdoc;
use itertools::Itertools;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::datatypes::attachment::ContentId;
use proton_mail_common::datatypes::attachment::MimeType;
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
    // This test fetches a message body twice and expects it to be called only once

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = message_body_test_params();

    let message = message_body_test_message_simple();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    // Will be called only once
    ctx.mock_get_messages_total_expect(vec![message.metadata.clone()], 1, 1)
        .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

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

    // We fetch it twice and expect it to be the same, with only 1 req being actually made.
    let decrypted_body = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    let decrypted_body2 = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    let body = tether
        .query_value::<_, String>(
            formatdoc!(
                "
            SELECT body as value FROM message_body WHERE
                message_id = {}
    ",
                saved_message.local_id.unwrap()
            ),
            vec![],
        )
        .await
        .unwrap();

    assert_eq!(decrypted_body.body, body);
    assert_eq!(decrypted_body.body, TEST_MESSAGE_BODY_DECRYPTED);
    assert_eq!(decrypted_body2.body, decrypted_body.body);
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

    let message = message_body_test_message_mime();

    let params = message_body_test_params();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

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

    // Action:
    //   * Get message body and PGP attachments
    let decrypted_message = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    let Err(MailContextError::App(AppError::UnknownCid(_, cids))) = decrypted_message
        .get_embedded_attachment(&user_ctx, &ContentId::from("fail"))
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
    let pgp_attachments = decrypted_message
        .metadata
        .attachments
        .into_iter()
        .collect_vec();

    assert_eq!(pgp_attachments.len(), 3);

    assert_eq!(pgp_attachments[0].filename, "attachment1.txt");
    assert_eq!(pgp_attachments[0].mime_type, MimeType::text_plain());

    let data = pgp_attachments[0]
        .content_data(&user_ctx, &mut tether)
        .await
        .unwrap();
    assert_eq!(data, b"attachment1");

    assert_eq!(pgp_attachments[1].filename, "attachment2.txt");
    assert_eq!(pgp_attachments[1].mime_type, MimeType::text_plain());
    let data = pgp_attachments[1]
        .content_data(&user_ctx, &mut tether)
        .await
        .unwrap();
    assert_eq!(data, b"attachment2");

    assert_eq!(
        pgp_attachments[2].filename,
        "OpenPGP_0x46F0FA708D336220.asc"
    );
    assert_eq!(
        pgp_attachments[2].mime_type,
        MimeType::from_str("application/pgp-keys").unwrap()
    );
    let data = pgp_attachments[2]
        .content_data(&user_ctx, &mut tether)
        .await
        .unwrap();
    assert_eq!(data, TEST_MESSAGE_BODY_MIME_SIGNATURE.as_bytes());
}

#[tokio::test]
async fn mailbox_message_retains_pgp_attachments() {
    // Setup:
    //   * Create a message encrypted with MIME format
    //     + Contains 2 attachments
    //     + OpenPGP public key

    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let message = message_body_test_message_mime();

    let params = message_body_test_params();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.mock_get_messages(vec![message.metadata.clone()]).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

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

    // Before decrypting the message there are no attachments yet because all are pgp
    assert!(saved_message.attachments_metadata.is_empty());

    // Action:
    //   * Get message body and PGP attachments
    let decrypted_message = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    // Now they exist :)
    assert_eq!(decrypted_message.metadata.attachments.len(), 3);

    let Err(MailContextError::App(AppError::UnknownCid(_, cids))) = decrypted_message
        .get_embedded_attachment(&user_ctx, &ContentId::from("fail"))
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
    let pgp_attachments = decrypted_message.metadata.attachments.iter().collect_vec();

    assert_eq!(pgp_attachments.len(), 3);

    let saved_message_2 = Message::load(1.into(), &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert_eq!(saved_message_2.attachments_metadata.len(), 3);
    let decrypted_message_2 = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    // It should retain the attachments.
    assert_eq!(decrypted_message.metadata, decrypted_message_2.metadata);
}
