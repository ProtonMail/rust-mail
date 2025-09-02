use indoc::formatdoc;
use itertools::Itertools;
use proton_core_api::services::proton::{LabelId, UserId};
use proton_mail_common::datatypes::attachment::ContentId;
use proton_mail_common::datatypes::attachment::MimeType;
use proton_mail_common::datatypes::message_banner::MessageBanner;
use proton_mail_common::datatypes::{AttachmentMetadata, SystemLabelId};
use proton_mail_common::models::MessageMimeType;
use proton_mail_common::models::{Attachment, Message};
use proton_mail_common::test_utils::message_body::{
    TEST_MESSAGE_BODY_DECRYPTED, TEST_MESSAGE_BODY_MIME_DECRYPTED,
    TEST_MESSAGE_BODY_MIME_SIGNATURE, TEST_USER_ID, message_body_test_message_mime,
    message_body_test_message_simple, message_body_test_params, message_body_test_user_secret,
};
use proton_mail_common::test_utils::test_context::MailTestContext;
use proton_mail_common::{AppError, MailContextError, Mailbox};
use stash::orm::Model;
use std::collections::HashSet;
use std::str::FromStr;

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
    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();
    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
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
                saved_message.id()
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

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();

    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let mut tether = user_ctx.user_stash().connection().await.unwrap();

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
    assert_eq!(decrypted_message.mime_type, MessageMimeType::TextPlain);

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

    // ---

    assert_eq!(pgp_attachments[1].filename, "attachment2.txt");
    assert_eq!(pgp_attachments[1].mime_type, MimeType::text_plain());

    let data = pgp_attachments[1]
        .content_data(&user_ctx, &mut tether)
        .await
        .unwrap();

    assert_eq!(data, b"attachment2");

    // ---

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

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();

    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let mut tether = user_ctx.user_stash().connection().await.unwrap();

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

    // Check attachments with disposition attachment are properly linked.
    let linked_attachments = Attachment::for_message(saved_message.id(), &tether)
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.id())
        .collect::<HashSet<_>>();

    assert_eq!(linked_attachments.len(), 3);

    for attachment in &decrypted_message.metadata.attachments {
        assert!(linked_attachments.contains(&attachment.id()));
    }

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
    assert_eq!(decrypted_message.mime_type, MessageMimeType::TextPlain);

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

#[tokio::test]
async fn pgp_mime_attachments_retrievable_via_get_attachments() {
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

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();

    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let mut tether = user_ctx.user_stash().connection().await.unwrap();

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

    let pgp_attachments = decrypted_message
        .metadata
        .attachments
        .into_iter()
        .collect_vec();

    assert_eq!(pgp_attachments.len(), 3);
    assert_eq!(pgp_attachments[0].filename, "attachment1.txt");
    assert_eq!(pgp_attachments[0].mime_type, MimeType::text_plain());

    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    for (index, attachment) in pgp_attachments.into_iter().enumerate() {
        let data = Attachment::get_attachment(&user_ctx, attachment.local_id.unwrap(), &mut tether)
            .await
            .unwrap_or_else(|_| panic!("failed to get attachment {index}"));

        assert_eq!(
            data.attachment_metadata,
            AttachmentMetadata::from(attachment),
            "Attachment {index} does not match"
        )
    }
}

#[tokio::test]
async fn message_body_failed_to_decrypt() {
    // This test fetches a message body twice and expects it to be called only once

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = message_body_test_params();
    let mut message = message_body_test_message_simple();

    message.body.body = "RANDOM CONTENT -- WON'T DECRYPT".into();

    ctx.setup_user(params.clone()).await;

    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (saved_message, decrypted_body) = Message::force_sync_message_and_body(
        &user_ctx,
        message.metadata.id.clone(),
        false,
        &mut tether,
    )
    .await
    .unwrap();

    assert_eq!(decrypted_body.body, message.body.body);
    assert!(decrypted_body.failed_to_decrypt());

    // check it is loaded correctly from the cache.
    let decrypted_body = saved_message
        .fetch_message_body(&user_ctx, &mut tether)
        .await
        .unwrap();

    assert_eq!(decrypted_body.body, message.body.body);
    assert_eq!(decrypted_body.mime_type, MessageMimeType::TextHtml);
    assert!(decrypted_body.failed_to_decrypt());

    let body_output = decrypted_body
        .transformed("", Default::default(), &tether)
        .await;

    assert!(
        body_output
            .body_banners
            .contains(&MessageBanner::UnableToDecrypt)
    );

    insta::assert_snapshot!(body_output.body);
}
