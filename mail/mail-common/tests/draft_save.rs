mod drafts_common;
use drafts_common::*;
use itertools::Itertools;
use proton_api_core::consts::{CoreBundle, Mail};
use proton_api_core::services::proton::common::ApiErrorInfo;
use proton_api_core::services::proton::{LabelId, UserId};
use proton_api_mail::services::proton::prelude::AttachmentId;
use proton_api_mail::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftRecipient,
};
use proton_api_mail::services::proton::response_data::MessageFlags;
use proton_api_mail::services::proton::response_data::{Disposition, MessageAttachment};
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_mail_common::MailContextError;
use proton_mail_common::datatypes::{MimeType, SystemLabelId};
use proton_mail_common::decrypted_message::DecryptedMessageBody;
use proton_mail_common::draft::{Draft, DraftSyncStatus, Error, OpenError, ReplyMode};
use proton_mail_common::models::{
    Attachment, Conversation, DraftMetadata, DraftSendResult, DraftSendResultOrigin, Message,
};
use proton_mail_test_utils::message_body::*;
use proton_mail_test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use uuid::Uuid;

#[tokio::test]
async fn create_empty_draft() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = draft_message();

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params,
        None,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    // Add some other label ids to this message to make sure they are skipped.
    message.metadata.label_ids.push(LabelId::starred());

    // Draft open always loads message from remote.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Load the draft.
    let tether = user_ctx.user_stash().connection();
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    let draft_conversation_id = draft.conversation_id(&tether).await.unwrap().unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(draft_message.remote_id, Some(message.metadata.id));

    // Check the draft has the draft label.
    assert_eq!(draft_message.label_ids.len(), 3);
    assert!(draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(draft_message.label_ids.contains(&LabelId::all_drafts()));
    assert!(draft_message.label_ids.contains(&LabelId::all_mail()));
    assert!(!draft_message.label_ids.contains(&LabelId::starred()));
    assert!(draft_message.is_draft());

    // Local conversation id should have been assigned.
    assert!(draft_message.local_conversation_id.is_some());
    assert_eq!(
        draft_conversation_id,
        draft_message.local_conversation_id.unwrap(),
    );

    // Loading the message body should not trigger any network requests.
    let message_body_metadata = Message::message_body(&user_ctx, draft_message.local_id.unwrap())
        .await
        .unwrap();

    assert!(message_body_metadata.metadata.attachments.is_empty());

    let conversation =
        Conversation::find_by_id(draft_message.local_conversation_id.unwrap(), &tether)
            .await
            .unwrap()
            .unwrap();

    // Conversation remote id has been set.
    assert_eq!(
        conversation.remote_id.unwrap(),
        message.metadata.conversation_id
    );
    // Conversation should also have the draft label.
    assert!(
        conversation
            .labels
            .iter()
            .any(|l| { l.remote_label_id == LabelId::drafts().into() })
    );

    // Opening this draft should work;
    Draft::open(&user_ctx, draft_message_id).await.unwrap();
}

#[tokio::test]
async fn create_empty_draft_and_save_twice() {
    // Create a new draft, save once to create, save again to trigger
    // update on server.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = draft_message();
    message.metadata.label_ids.push(LabelId::drafts());

    let new_subject = "My New Subject";
    let new_body = "<html><head></head><body>Hello world</body></html>";
    let new_to_list = new_recipient_list_with_single_address("to@list.info".to_owned());
    let new_cc_list = new_recipient_list_with_single_address("cc@list.info".to_owned());
    let new_bcc_list = new_recipient_list_with_single_address("bcc@list.info".to_owned());

    let mut updated_message = message.clone();
    updated_message.metadata.subject = new_subject.into();
    updated_message.metadata.to_list = new_to_list
        .to_message_recipients()
        .into_iter()
        .map_into()
        .collect();
    updated_message.metadata.cc_list = new_cc_list
        .to_message_recipients()
        .into_iter()
        .map_into()
        .collect();
    updated_message.metadata.bcc_list = new_bcc_list
        .to_message_recipients()
        .into_iter()
        .map_into()
        .collect();
    updated_message.body.body = r"
-----BEGIN PGP MESSAGE-----

wV4DGS71hsmM2EQSAQdApsxU764ePCfQ/bwfVTrxYKCvQIyxjIjaOQqg1lfn5VAw
yuGKKmEIA2WMImYHzbN8S2ZTPWuXRfckWme8E+OkuWSVWiPKnAxMHzjwrH3V944J
0sBIAY1Ckxnz3zkWyEebOupvGGj8257b+MrzPw9UyiQ+ND5bT7nKESgMtTlA2f0s
XpgZ4uycHvmtocbNR49E/tazWFidaIf1ZNAnpv6va8uxZZ2ddOf5u1SWY34D30Bu
f8Y3F9U7piAUl2gd1ZeZEsTsoIW95UKa+BY0THXFivcLKmoUcYNRrugLk6TlJhEX
beORgaHsf+TBYuXKvN1dtYmwzSifsi8pdMsK1WebfDQjdWRRSCs3FKi4yH9PkVUg
dJyN3/sZg/QCLSAKstzw1RgqWAoUdWL9p04IvSDmb7fwbUspBOpZMBZfJp6OfrHt
3HM6yy1bT6WQy/xGG9YZeeYZbuzC8tO8WqS/
=1xQJ
-----END PGP MESSAGE-----
"
    .to_owned();

    let expected_draft_params = expected_create_draft_params();
    let expected_update_draft_params = {
        let mut params = expected_create_draft_params();
        params.subject = new_subject.to_owned();
        params.to_list = new_to_list
            .to_message_recipients()
            .into_iter()
            .map(|v| DraftRecipient {
                address: v.address,
                name: v.name,
                group: v.group.into_option(),
            })
            .collect();
        params.cc_list = new_cc_list
            .to_message_recipients()
            .into_iter()
            .map(|v| DraftRecipient {
                address: v.address,
                name: v.name,
                group: v.group.into_option(),
            })
            .collect();
        params.bcc_list = new_bcc_list
            .to_message_recipients()
            .into_iter()
            .map(|v| DraftRecipient {
                address: v.address,
                name: v.name,
                group: v.group.into_option(),
            })
            .collect();
        params
    };

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params,
        None,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    ctx.mock_update_draft(
        updated_message.metadata.id.clone(),
        expected_update_draft_params,
        updated_message.clone(),
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    // Draft open always loads message from remote.
    ctx.mock_get_message(&updated_message.metadata.id, updated_message.clone())
        .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Update the draft
    draft.subject = new_subject.to_owned();
    draft.set_body(new_body.to_owned());
    draft.to_list = new_to_list.clone();
    draft.cc_list = new_cc_list.clone();
    draft.bcc_list = new_bcc_list.clone();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();
    user_ctx.execute_all_send_actions().await.unwrap();

    let tether = user_ctx.user_stash().connection();
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    // Opening the draft and check if all the information is up to date
    let (draft, _) = Draft::open(&user_ctx, draft_message_id).await.unwrap();
    assert_eq!(draft.body(), new_body);
    assert_eq!(draft.subject, new_subject);
    assert_eq!(draft.to_list, new_to_list);
    assert_eq!(draft.cc_list, new_cc_list);
    assert_eq!(draft.bcc_list, new_bcc_list);
}

#[tokio::test]
async fn create_draft_reply_without_body_is_error() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_simple();
    remote_existing_message.metadata.id = "Fancy Remote Id".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    ctx.setup_user(params.clone()).await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let (mut existing_message, _, _) = Message::from_api_data(remote_existing_message, &tether)
        .await
        .unwrap();
    let tx = tether.transaction().await.unwrap();
    existing_message.save(&tx).await.unwrap();
    tx.commit().await.unwrap();
    let existing_message = existing_message;

    ctx.catch_all().await;

    // Create draft.
    let result = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        ReplyMode::Sender,
        true,
    )
    .await;

    assert!(matches!(
        result,
        Err(MailContextError::Draft(Error::Open(
            OpenError::MessageBodyMissing(_)
        )))
    ));
}

#[tokio::test]
async fn create_draft_reply_should_fail_for_drafts() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_simple();
    remote_existing_message.metadata.id = "Fancy Remote Id".into();
    // is draft checks whether received or sent flags are present
    // set to empty to consider it as a draft.
    remote_existing_message.metadata.flags = MessageFlags::empty();

    ctx.setup_user(params.clone()).await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let (mut existing_message, _, _) = Message::from_api_data(remote_existing_message, &tether)
        .await
        .unwrap();
    let tx = tether.transaction().await.unwrap();
    existing_message.save(&tx).await.unwrap();
    tx.commit().await.unwrap();
    let existing_message = existing_message;

    ctx.catch_all().await;

    // Create draft.
    let result = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        ReplyMode::Sender,
        true,
    )
    .await;

    assert!(matches!(
        result,
        Err(MailContextError::Draft(Error::Open(
            OpenError::ReplyOrForwardToDraft(_)
        )))
    ));
}

#[tokio::test]
async fn metadata_is_create_for_existing_not_opened_draft() {
    // Simulate opening a draft that was created in another session. We should
    // create metadata for this message so the `Save` action works correctly.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = draft_message();
    message.metadata.label_ids.push(LabelId::drafts());

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message_with_expected(&message.metadata.id, message.clone(), 2)
        .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let mut message = Message::from_api_metadata(message.metadata, &tether)
        .await
        .unwrap();

    // Save message.
    let tx = tether.transaction().await.unwrap();
    message.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    assert!(
        DraftMetadata::find_by_message_id(message.local_id.unwrap(), &tether)
            .await
            .unwrap()
            .is_none()
    );

    // Create draft.
    let (draft, _) = Draft::open(&user_ctx, message.local_id.unwrap())
        .await
        .unwrap();

    let draft_by_message_id = DraftMetadata::find_by_message_id(message.local_id.unwrap(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(draft.metadata_id, draft_by_message_id.id.unwrap());
    drop(draft);

    // Opening this draft again should not create new metadata;
    let (draft, _) = Draft::open(&user_ctx, message.local_id.unwrap())
        .await
        .unwrap();

    assert_eq!(draft.metadata_id, draft_by_message_id.id.unwrap());
}

#[tokio::test]
async fn create_draft_reply_html() {
    let draft_body = create_draft_reply_impl(MimeType::TextHtml, ReplyMode::Sender).await;
    insta::assert_snapshot!(draft_body.body)
}

#[tokio::test]
async fn create_draft_reply_plain_text() {
    let draft_body = create_draft_reply_impl(MimeType::TextPlain, ReplyMode::Sender).await;
    insta::assert_snapshot!(draft_body.body)
}

#[tokio::test]
async fn create_draft_reply_inherits_only_inline_attachments() {
    let draft_body = create_draft_reply_impl(MimeType::TextPlain, ReplyMode::Sender).await;
    assert_eq!(draft_body.metadata.attachments.len(), 1);
    assert_eq!(draft_body.metadata.attachments.len(), 1);
    let attachment = draft_body.metadata.attachments.first().unwrap();
    let inline_attachment = gen_inline_attachment();
    compare_inline_attachment(attachment, inline_attachment);
}

#[tokio::test]
async fn draft_save_failure_creates_send_result_with_correct_origin() {
    // Create a new draft, save once to create, save again to trigger
    // update on server.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.label_ids.push(LabelId::drafts());

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft_failure(
        expected_draft_params,
        None,
        None,
        DraftAttachmentKeyPackets::new(),
        CoreBundle::AppVersionInvalid as u32,
    )
    .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap_err();
    let tether = user_ctx.user_stash().connection();

    let send_result =
        DraftSendResult::find_by_id(draft.message_id(&tether).await.unwrap().unwrap(), &tether)
            .await
            .unwrap()
            .unwrap();
    assert!(!send_result.is_success());
    assert_eq!(send_result.origin, DraftSendResultOrigin::Save);
}

#[tokio::test]
async fn create_draft_forward_inherits_all_attachments() {
    let draft_body = create_draft_reply_impl(MimeType::TextPlain, ReplyMode::Forward).await;
    assert_eq!(draft_body.metadata.attachments.len(), 2);

    let attachment_1 = &draft_body.metadata.attachments[1];
    let attachment_2 = &draft_body.metadata.attachments[0];

    let inline_attachment = gen_inline_attachment();
    let normal_attachment = gen_normal_attachment();

    compare_inline_attachment(attachment_1, inline_attachment);
    assert_ne!(
        attachment_2.remote_id().unwrap().as_str(),
        normal_attachment.id.as_str()
    );
    assert_eq!(
        attachment_2.disposition,
        normal_attachment.disposition.into()
    );
    assert_eq!(attachment_2.filename, normal_attachment.name);
    assert_eq!(attachment_2.size, normal_attachment.size);
}

fn compare_inline_attachment(attachment: &Attachment, inline_attachment: MessageAttachment) {
    assert_ne!(
        attachment.remote_id().unwrap().as_str(),
        inline_attachment.id.as_str(),
        "Expected the same remote id"
    );
    assert_eq!(attachment.disposition, inline_attachment.disposition.into());
    assert_eq!(attachment.filename, inline_attachment.name);
    assert_eq!(attachment.size, inline_attachment.size);
    assert_eq!(attachment.content_id, inline_attachment.headers.content_id);
}

async fn create_draft_reply_impl(
    mime_type: MimeType,
    reply_mode: ReplyMode,
) -> DecryptedMessageBody {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params_with_mime_type(mime_type);

    // Create one message we can reply to.
    let mut remote_existing_message = draft_message_with_attachments();
    remote_existing_message.metadata.sender.address = "me@proton.me".to_owned();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    remote_existing_message.body.attachments.reverse();

    ctx.setup_user(params.clone()).await;
    let user_ctx = ctx.mail_user_context().await;

    let mut tether = user_ctx.user_stash().connection();
    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();
    let tx = tether.transaction().await.unwrap();
    existing_message.save(&tx).await.unwrap();
    tx.commit().await.unwrap();
    let existing_message = existing_message;

    let expected_draft_params =
        expected_create_reply_draft_params(&existing_message, mime_type, reply_mode);
    let mut message = draft_message();
    message.body.attachments = remote_existing_message.body.attachments.clone();
    if reply_mode != ReplyMode::Forward {
        message
            .body
            .attachments
            .retain(|a| a.disposition == Disposition::Inline)
    }
    // Inherited attachments get new remote ids
    for attachment in &mut message.body.attachments {
        attachment.id = AttachmentId::from(Uuid::new_v4().to_string());
    }

    let key_packets = DraftAttachmentKeyPackets::from_iter(
        remote_existing_message
            .body
            .attachments
            .iter()
            .filter(|a| {
                if reply_mode == ReplyMode::Forward {
                    true
                } else {
                    a.disposition == Disposition::Inline
                }
            })
            .map(|a| (a.id.clone(), a.key_packets.clone())),
    );
    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;
    ctx.mock_create_draft(
        expected_draft_params,
        Some(DraftAction::from(reply_mode)),
        message.clone(),
        Some(existing_message.remote_id.clone().unwrap()),
        key_packets,
    )
    .await;
    // Decrypted message downloads attachments.
    for attachment in &message.body.attachments {
        ctx.mock_maybe_get_attachment_data(attachment.id.clone(), vec![])
            .await;
    }
    // Opening a draft always syncs the message.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.catch_all().await;

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(&user_ctx, existing_message.remote_id.unwrap(), false)
        .await
        .unwrap();

    let tx = tether.transaction().await.unwrap();
    // Insert attachment data into the cache.
    for attachment in &remote_existing_message.body.attachments {
        let local_attachment_id = Attachment::remote_id_counterpart(attachment.id.clone(), &tx)
            .await
            .unwrap()
            .unwrap();
        Attachment::store_in_cache(
            &user_ctx,
            &attachment.name,
            local_attachment_id,
            attachment.name.as_bytes().to_vec(),
            &tx,
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    // Create draft.
    let mut draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        reply_mode,
        true,
    )
    .await
    .unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    let draft_conversation_id = draft.conversation_id(&tether).await.unwrap().unwrap();

    // Load the draft.
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    let sender_address = Address::find_by_remote_id(existing_message.remote_address_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(draft_message.remote_id, Some(message.metadata.id));

    // Sender address should not be repeated in replies or forward.
    assert!(
        !draft_message
            .to_list
            .value
            .iter()
            .any(|v| { v.address == sender_address.email })
    );
    assert!(
        !draft_message
            .cc_list
            .value
            .iter()
            .any(|v| { v.address == sender_address.email })
    );

    // Local conversation id match the source message,
    assert_eq!(
        draft_message.local_conversation_id.unwrap(),
        existing_message.local_conversation_id.unwrap(),
    );
    assert_eq!(
        draft_conversation_id,
        existing_message.local_conversation_id.unwrap(),
    );

    // Check the draft has the draft label.
    assert!(draft_message.label_ids.contains(&LabelId::drafts()));

    // Loading the message body should not trigger any network requests.
    let draft_body = Message::message_body(&user_ctx, draft_message.local_id.unwrap())
        .await
        .unwrap();

    let conversation =
        Conversation::find_by_id(draft_message.local_conversation_id.unwrap(), &tether)
            .await
            .unwrap()
            .unwrap();

    // Conversation remote id has been set.
    assert_eq!(
        conversation.remote_id.unwrap(),
        message.metadata.conversation_id
    );

    // Opening this draft should work;
    Draft::open(&user_ctx, draft_message_id).await.unwrap();

    draft_body
}

#[tokio::test]
async fn open_draft_sync_status_success() {
    // Check that open draft successfully reports synced status when synced from server.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let message = draft_message();

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params,
        None,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    // Draft open always loads message from remote.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Load the draft.
    let tether = user_ctx.user_stash().connection();
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    // Opening this draft should work;
    let (_, sync_status) = Draft::open(&user_ctx, draft_message_id).await.unwrap();
    assert_eq!(sync_status, DraftSyncStatus::Synced);
}

#[tokio::test]
async fn open_draft_sync_status_cached() {
    // Check that open draft reports cached status when we can't sync from server due to
    // network failure.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = draft_message();
    message.metadata.label_ids.push(LabelId::drafts());

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params,
        None,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    // Draft open always loads message from remote.
    ctx.mock_get_message_failure(
        &message.metadata.id,
        500,
        ApiErrorInfo {
            code: Mail::MessageUpdateDraftNotExist as u32,
            error: None,
            details: None,
        },
    )
    .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Load the draft.
    let tether = user_ctx.user_stash().connection();
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    // Opening this draft should work;
    let (_, sync_status) = Draft::open(&user_ctx, draft_message_id).await.unwrap();
    assert_eq!(sync_status, DraftSyncStatus::Cached);
}

#[tokio::test]
async fn open_new_draft_which_was_not_saved_on_server_should_not_report_cached_status() {
    // Check that open draft reports cached status when we can't sync from server due to
    // network failure.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.label_ids.push(LabelId::drafts());

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Load the draft.
    let tether = user_ctx.user_stash().connection();
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    // Opening this draft should work;
    let (_, sync_status) = Draft::open(&user_ctx, draft_message_id).await.unwrap();
    assert_eq!(sync_status, DraftSyncStatus::Synced);
}
