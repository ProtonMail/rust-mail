use super::drafts_common::*;
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, AsActionError, QueuedError};
use proton_core_api::consts::{CoreBundle, Mail};
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_api::services::proton::{AddressStatus, LabelId, UserId};
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::{Address, ModelExtension, ModelIdExtension};
use proton_crypto_inbox::attachment::KeyPackets;
use proton_mail_api::services::proton::common::ConversationId;
use proton_mail_api::services::proton::prelude::{
    AttachmentId, ContentDisposition, MessageAttachmentHeaders, NewAttachmentDisposition,
    NewAttachmentParams, NewAttachmentResponse, PostAttachmentResponse,
};
use proton_mail_api::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftRecipient,
};
use proton_mail_api::services::proton::response_data::MessageFlags;
use proton_mail_api::services::proton::response_data::{Disposition, MessageAttachment};
use proton_mail_common::MailContextError;
use proton_mail_common::datatypes::attachment::{ContentId, MimeType as AttachmentMimeType};
use proton_mail_common::datatypes::{
    AttachmentMetadata, MessageSender, MimeType, ParsedHeaders, RollbackItemType, SystemLabelId,
};
use proton_mail_common::decrypted_message::DecryptedMessageBody;
use proton_mail_common::draft::compose::DraftAddressValidationError;
use proton_mail_common::draft::{
    Draft, DraftActorOptions, DraftExpirationTime, DraftSyncStatus, Error, OpenError, ReplyMode,
    SaveError,
};
use proton_mail_common::models::{
    Attachment, AttachmentType, Conversation, DraftAttachmentMetadata, DraftAttachmentUploadState,
    DraftMetadata, DraftSendResult, DraftSendResultOrigin, Message, MessageBodyMetadata,
    RawMessageBody, RollbackItem,
};
use proton_mail_common::test_utils::message_body::*;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::UserDb;
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
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
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    // Add some other label ids to this message to make sure they are skipped.
    message.metadata.label_ids.push(LabelId::starred());

    // Draft open always loads message from remote.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Load the draft.
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let draft_message_id = draft.message_id().await.unwrap().unwrap();
    let draft_conversation_id = draft.conversation_id().await.unwrap().unwrap();

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
    assert_eq!(draft_message.expiration_time, UnixTimestamp::new(0));

    // Local conversation id should have been assigned.
    assert!(draft_message.local_conversation_id.is_some());
    assert_eq!(
        draft_conversation_id,
        draft_message.local_conversation_id.unwrap(),
    );

    // Loading the message body should not trigger any network requests.
    let message_body_metadata = Message::message_body(&user_ctx, draft_message.id())
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
        Some(DraftAttachmentKeyPackets::new()),
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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Update the draft
    draft.set_subject(new_subject.to_owned()).await.unwrap();
    draft.set_body(new_body.to_owned()).await.unwrap();
    draft
        .set_recipients(
            new_to_list.clone(),
            new_cc_list.clone(),
            new_bcc_list.clone(),
        )
        .await
        .unwrap();

    draft.save().await.unwrap();

    user_ctx.execute_all_send_actions().await.unwrap();

    let tether = user_ctx.user_stash().connection().await.unwrap();
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    // Opening the draft and check if all the information is up to date
    let (draft, _) = Draft::open(&user_ctx, draft_message_id).await.unwrap();
    let state = draft.state().await.unwrap();
    assert_eq!(state.body, new_body);
    assert_eq!(state.subject, new_subject);
    assert_eq!(state.to_list, new_to_list);
    assert_eq!(state.cc_list, new_cc_list);
    assert_eq!(state.bcc_list, new_bcc_list);

    let draft_conv_id = draft.conversation_id().await.unwrap().unwrap();

    let conv = Conversation::find_by_id(draft_conv_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(conv.subject, new_subject);
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
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) = Message::from_api_data(remote_existing_message, &tether)
        .await
        .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    // Create draft.
    let result = Draft::reply(&user_ctx, existing_message.id(), ReplyMode::Sender, true).await;

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
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) = Message::from_api_data(remote_existing_message, &tether)
        .await
        .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    // Create draft.
    let result = Draft::reply(&user_ctx, existing_message.id(), ReplyMode::Sender, true).await;

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

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut message = Message::from_api_metadata(message.metadata, &tether)
        .await
        .unwrap();

    // Save message.
    tether.tx(async |tx| message.save(tx).await).await.unwrap();

    assert!(
        DraftMetadata::find_by_message_id(message.id(), &tether)
            .await
            .unwrap()
            .is_none()
    );

    // Create draft.
    let (draft, _) = Draft::open(&user_ctx, message.id()).await.unwrap();

    let draft_by_message_id = DraftMetadata::find_by_message_id(message.id(), &tether)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(draft.metadata_id, draft_by_message_id.id.unwrap());

    drop(draft);

    // Opening this draft again should not create new metadata;
    let (draft, _) = Draft::open(&user_ctx, message.id()).await.unwrap();

    assert_eq!(draft.metadata_id, draft_by_message_id.id.unwrap());
}

#[tokio::test]
async fn create_draft_reply_html() {
    let draft_body = create_draft_reply_impl(MimeType::TextHtml, ReplyMode::Sender).await;
    insta::assert_snapshot!(draft_body.body)
}

#[tokio::test]
async fn create_draft_reply_with_alias() {
    // Check if we received the email on an alias it is set correctly
    // on the message.
    let alias_email = "rust_test+alias@proton.ch";
    create_draft_reply_with_override_impl(
        MimeType::TextHtml,
        ReplyMode::Sender,
        Some(alias_email.to_owned()),
    )
    .await;
}

#[tokio::test]
async fn create_draft_reply_inherits_only_inline_attachments() {
    let draft_body = create_draft_reply_impl(MimeType::TextHtml, ReplyMode::Sender).await;
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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap_err();

    let tether = user_ctx.user_stash().connection().await.unwrap();

    let send_result =
        DraftSendResult::find_by_id(draft.message_id().await.unwrap().unwrap(), &tether)
            .await
            .unwrap()
            .unwrap();

    assert!(!send_result.is_success());
    assert_eq!(send_result.origin, DraftSendResultOrigin::Save);
}

#[tokio::test]
async fn create_draft_forward_inherits_all_attachments() {
    let draft_body = create_draft_reply_impl(MimeType::TextHtml, ReplyMode::Forward).await;
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
    assert_eq!(
        attachment.content_id,
        inline_attachment.headers.content_id.map(ContentId::from)
    );
}

async fn create_draft_reply_impl(
    mime_type: MimeType,
    reply_mode: ReplyMode,
) -> DecryptedMessageBody {
    create_draft_reply_with_override_impl(mime_type, reply_mode, None).await
}

async fn create_draft_reply_with_override_impl(
    mime_type: MimeType,
    reply_mode: ReplyMode,
    alias_override: Option<String>,
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
    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.body.reply_to.address = "me@proton.me".into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    if let Some(alias_override) = &alias_override {
        remote_existing_message.body.parsed_headers.insert(
            "X-Original-To".to_owned(),
            serde_json::Value::String(alias_override.clone()),
        );
    }

    remote_existing_message.body.attachments.reverse();
    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    let mut expected_draft_params =
        expected_create_reply_draft_params(&existing_message, mime_type, reply_mode);

    // override alias if present
    if let Some(alias_override) = &alias_override {
        expected_draft_params.sender.address = alias_override.clone().into();
    }

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
        None,
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

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Insert attachment data into the cache.
            for attachment in &remote_existing_message.body.attachments {
                let local_attachment_id =
                    Attachment::remote_id_counterpart(attachment.id.clone(), tx)
                        .await
                        .unwrap()
                        .unwrap();

                Attachment::store_in_cache(
                    &user_ctx,
                    &attachment.name,
                    local_attachment_id,
                    attachment.name.as_bytes().to_vec(),
                    tx,
                )
                .await
                .unwrap();
            }
            Ok(())
        })
        .await
        .unwrap();

    // Create draft.
    let draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        reply_mode,
        true,
    )
    .await
    .unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let draft_message_id = draft.message_id().await.unwrap().unwrap();
    let draft_conversation_id = draft.conversation_id().await.unwrap().unwrap();

    // Load the draft.
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    let sender_address = Address::find_by_remote_id(existing_message.remote_address_id, &tether)
        .await
        .unwrap()
        .unwrap();

    if let Some(alias_override) = alias_override {
        assert_eq!(draft.sender().await.unwrap(), alias_override);
    }

    assert_eq!(draft_message.remote_id, Some(message.metadata.id));

    // Sender address should not be repeated in replies or forward.
    assert!(
        !draft_message
            .to_list
            .value
            .iter()
            .any(|v| { v.address.as_clear_text_str() == sender_address.email })
    );
    assert!(
        !draft_message
            .cc_list
            .value
            .iter()
            .any(|v| { v.address.as_clear_text_str() == sender_address.email })
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
    let draft_body = Message::message_body(&user_ctx, draft_message.id())
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
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    // Draft open always loads message from remote.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Load the draft.
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

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
        Some(DraftAttachmentKeyPackets::new()),
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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Load the draft.
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Load the draft.
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    // Opening this draft should work;
    let (_, sync_status) = Draft::open(&user_ctx, draft_message_id).await.unwrap();

    assert_eq!(sync_status, DraftSyncStatus::Synced);
}

#[tokio::test]
async fn new_draft_conversation_remote_id_updated_externally() {
    // It is possible that the draft conversation gets its remote id assigned before the draft
    // is able to update it by itself, this leads to a db constraint error.
    // This usually happens due to the prefetcher, but can also happend because of the event loop.
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
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    // Add some other label ids to this message to make sure they are skipped.
    message.metadata.label_ids.push(LabelId::starred());

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Load the draft.
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let draft_message_id = draft.message_id().await.unwrap().unwrap();
    let draft_conversation_id = draft.conversation_id().await.unwrap().unwrap();

    // Simulate the new conversation being created via other means.
    let mut fetched_conv = Conversation::find_by_id(draft_conversation_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(fetched_conv.remote_id.is_none());

    fetched_conv.remote_id = Some(message.metadata.conversation_id.clone());
    fetched_conv.local_id = None;

    tether
        .tx(async |tx| fetched_conv.save(tx).await)
        .await
        .unwrap();

    assert_ne!(fetched_conv.id(), draft_conversation_id);

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    // Our conversation should be not be present.
    assert!(
        Conversation::find_by_id(draft_conversation_id, &tether)
            .await
            .unwrap()
            .is_some()
    );

    // Local conversation id should have been assigned with the existing message.
    assert_eq!(
        draft_message.local_conversation_id.unwrap(),
        draft_conversation_id,
    );

    assert_eq!(
        draft.conversation_id().await.unwrap().unwrap(),
        draft_conversation_id
    );
}

#[tokio::test]
async fn already_sent_error_move_draft_to_sent_and_schedules_rollback() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let message = draft_message();
    let expected_draft_params = expected_create_draft_params();
    let mut updated_draft_params = expected_draft_params.clone();

    updated_draft_params.subject = "Modified".to_string();

    ctx.setup_user(params.clone()).await;

    ctx.mock_create_draft(
        expected_draft_params,
        None,
        message.clone(),
        None,
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    ctx.mock_update_draft_failure(
        message.metadata.id.clone(),
        updated_draft_params,
        DraftAttachmentKeyPackets::new(),
        ApiErrorInfo {
            code: Mail::MessageAlreadySent as u32,
            error: None,
            details: None,
        },
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Load the draft.
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    draft.set_subject("Modified".to_owned()).await.unwrap();

    draft.save().await.unwrap();

    let err = user_ctx.execute_all_send_actions().await.unwrap_err();

    let QueuedError::Action(err, _) = err else {
        panic!("unexpected error")
    };

    let err = err
        .as_action_error::<proton_mail_common::actions::draft::Save, UserDb>()
        .unwrap();

    assert!(matches!(
        err,
        ActionError::Action(MailContextError::Draft(Error::Save(SaveError::AlreadySent)))
    ));

    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert_eq!(draft_message.remote_id, Some(message.metadata.id.clone()));
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(!draft_message.label_ids.contains(&LabelId::all_drafts()));
    assert!(draft_message.label_ids.contains(&LabelId::sent()));
    assert!(!draft_message.is_draft());

    let rollback_item = RollbackItem::find_by_id(message.metadata.id.clone().into_inner(), &tether)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(rollback_item.item_type, RollbackItemType::Message);
}

#[tokio::test]
async fn attach_public_key_empty_draft() {
    // We don't need to validate that the full attachment pipeline works, only that
    // the attachment was created and is pending state.
    // The rest of the behavior is validated with the attachment upload tests.
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params();

    params.mail_settings.as_mut().unwrap().attach_public_key = true;
    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    let draft_attachments =
        DraftAttachmentMetadata::attachment_for_draft(draft.metadata_id, &tether)
            .await
            .unwrap();

    assert_eq!(draft_attachments.len(), 1);
    assert!(draft_attachments[0].is_public_key_attachment());
    assert_eq!(
        draft_attachments[0].disposition,
        Disposition::Attachment.into()
    );
}

#[tokio::test]
async fn attach_public_key_reply_draft() {
    // If the reply does not have the public key it should be added.
    prepare_draft_reply_attach_public_key(false, ReplyMode::Forward, async |draft, tether| {
        let draft_attachments =
            DraftAttachmentMetadata::attachment_for_draft(draft.metadata_id, tether)
                .await
                .unwrap();

        assert_eq!(draft_attachments.len(), 1);
        assert!(draft_attachments[0].is_public_key_attachment());
        assert_eq!(
            draft_attachments[0].disposition,
            Disposition::Attachment.into()
        );

        let attachment_metadata =
            DraftAttachmentMetadata::find_by_id(draft_attachments[0].id(), tether)
                .await
                .unwrap()
                .unwrap();

        // Upload state should be pending since this was created.
        assert_eq!(
            attachment_metadata.state(),
            DraftAttachmentUploadState::Pending
        );
        assert!(attachment_metadata.is_public_key)
    })
    .await;
}

#[tokio::test]
async fn attach_public_key_reply_draft_does_not_duplicate_if_already_there() {
    // If an existing public key is already present in the attachment list of the reply,
    // we should not add it again.
    prepare_draft_reply_attach_public_key(true, ReplyMode::Forward, async |draft, tether| {
        let draft_attachments =
            DraftAttachmentMetadata::attachment_for_draft(draft.metadata_id, tether)
                .await
                .unwrap();

        assert_eq!(draft_attachments.len(), 1);
        assert!(draft_attachments[0].is_public_key_attachment());
        assert_eq!(
            draft_attachments[0].disposition,
            Disposition::Attachment.into()
        );

        let attachment_metadata =
            DraftAttachmentMetadata::find_by_id(draft_attachments[0].id(), tether)
                .await
                .unwrap()
                .unwrap();

        // Upload state should be uploaded since it already exists.
        assert_eq!(
            attachment_metadata.state(),
            DraftAttachmentUploadState::Uploaded
        );
        assert!(attachment_metadata.is_public_key)
    })
    .await;
}

#[tokio::test]
async fn open_draft_resets_password() {
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
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    // Add some other label ids to this message to make sure they are skipped.
    message.metadata.label_ids.push(LabelId::starred());

    // Draft open always loads message from remote.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .set_password("foo_bar_and_some", Some("foo".to_string()))
        .await
        .unwrap();

    draft.save().await.unwrap();

    // Load the draft.
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Opening this draft should work;
    let (draft, status) = Draft::open(&user_ctx, draft_message_id).await.unwrap();

    assert_eq!(status, DraftSyncStatus::Synced);
    assert!(draft.get_password().await.unwrap().is_none());

    let metadata = DraftMetadata::find_by_id(draft.metadata_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(metadata.password_hint.is_none());
    assert!(metadata.password.is_none());
    assert_eq!(metadata.expiration_time(), DraftExpirationTime::Never);
}

#[tokio::test]
async fn create_draft_reply_with_invalid_address_produces_address_validation_error() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params_with_mime_type(MimeType::TextHtml);
    params.addresses[0].status = AddressStatus::Disabled;

    // Create one message we can reply to.
    let mut remote_existing_message = draft_message_with_attachments();

    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.body.reply_to.address = "me@proton.me".into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();

    // Create draft.
    let draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        ReplyMode::Sender,
        true,
    )
    .await
    .unwrap();

    assert_eq!(draft.address_id().await.unwrap(), params.addresses[1].id);

    let validation_result = draft
        .take_address_validation_result()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(validation_result.email, params.addresses[0].email);
    assert_eq!(
        validation_result.error,
        DraftAddressValidationError::Disabled
    );
}

#[tokio::test]
async fn open_draft_catches_invalid_address() {
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
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    // Draft open always loads message from remote.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Change the address to invalid
    let mut address =
        Address::find_by_remote_id(draft.address_id().await.unwrap().clone(), &tether)
            .await
            .unwrap()
            .unwrap();

    address.status = AddressStatus::Disabled.into();

    tether.tx(async |tx| address.save(tx).await).await.unwrap();

    // Load the draft.
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    // Opening this draft again should trigger the address change invalidation
    let (draft, sync_status) = Draft::open(&user_ctx, draft_message_id).await.unwrap();

    assert_eq!(sync_status, DraftSyncStatus::Synced);
    assert_ne!(
        draft.address_id().await.unwrap(),
        address.remote_id.unwrap()
    );

    let validation_result = draft
        .take_address_validation_result()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(validation_result.email, address.email);
    assert_eq!(
        validation_result.error,
        DraftAddressValidationError::Disabled
    );
}

#[tokio::test]
async fn open_draft_detects_sender_alias() {
    // Check that open draft successfully reports synced status when synced from server.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let alias_address = "rust_test_to+alias@proton.ch";
    let params = draft_test_params();
    let mut message = draft_message();
    message.metadata.sender.address = alias_address.to_owned().into();

    ctx.setup_user(params.clone()).await;

    // Draft open always loads message from remote.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let local_address_id = Address::remote_id_counterpart(params.addresses[0].id.clone(), &tether)
        .await
        .unwrap()
        .unwrap();

    let mut message = Message {
        local_id: None,
        remote_id: Some(message.metadata.id.clone()),
        local_conversation_id: None,
        remote_conversation_id: Some(ConversationId::from("CONV")),
        local_address_id,
        remote_address_id: params.addresses[0].id.clone(),
        attachments_metadata: vec![],
        cc_list: Default::default(),
        bcc_list: Default::default(),
        deleted: false,
        location: None,
        expiration_time: Default::default(),
        external_id: None,
        flags: Default::default(),
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![LabelId::drafts()],
        num_attachments: 0,
        display_order: 0,
        sender: MessageSender {
            address: alias_address.to_owned().into(),
            bimi_selector: None,
            display_sender_image: false,
            is_proton: false,
            is_simple_login: false,
            name: Default::default(),
        },
        size: 0,
        snooze_time: Default::default(),
        subject: "".to_string(),
        time: Default::default(),
        to_list: Default::default(),
        unread: false,
        custom_labels: vec![],
    };
    let mut message_body_metadata = MessageBodyMetadata {
        local_message_id: None,
        remote_message_id: message.remote_id.clone(),
        header: "".to_string(),
        mime_type: MimeType::TextHtml,
        parsed_headers: ParsedHeaders {
            headers: HashMap::from([(
                "X-Original-To".to_owned(),
                serde_json::Value::String(alias_address.to_owned()),
            )]),
        },
        attachments: vec![],
        reply_to: Default::default(),
        reply_tos: vec![],
    };

    tether
        .tx(async |tx| {
            message.save(tx).await.unwrap();
            message_body_metadata.save(tx).await.unwrap();

            RawMessageBody::local_draft("Hello world")
                .store(message.id(), tx)
                .await
        })
        .await
        .unwrap();

    let (draft, sync_status) = Draft::open(&user_ctx, message.id()).await.unwrap();

    assert_eq!(sync_status, DraftSyncStatus::Synced);
    assert_eq!(draft.sender().await.unwrap(), alias_address);
    let addresses = draft.sender_addresses().await.unwrap();
    // Alias address should be at the top.
    assert_eq!(addresses[0].email, alias_address);
}

#[tokio::test]
async fn auto_save_draft() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params();
    if let Some(setting) = params.mail_settings.as_mut() {
        setting.draft_mime_type = MimeType::TextPlain.into();
    }

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    // Create draft.
    let draft = Draft::empty_ex(
        &user_ctx,
        DraftActorOptions {
            auto_save_every: Some(Duration::from_millis(500)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    draft.set_body("foo".into()).await.unwrap();
    let msg_id = draft.message_id().await.unwrap().unwrap();
    draft.set_body("foo_bar".into()).await.unwrap();
    // Body remains unchanged
    let body = Message::message_body(&user_ctx, msg_id).await.unwrap();
    assert_eq!(body.body, "foo");
    tokio::time::sleep(Duration::from_millis(500)).await;
    // We have elapsed the save interval, we should see changes now.
    draft.set_body("foo_barish".into()).await.unwrap();
    let body = Message::message_body(&user_ctx, msg_id).await.unwrap();
    assert_eq!(body.body, "foo_barish");
}

async fn prepare_draft_reply_attach_public_key(
    pre_attach_public_key: bool,
    reply_mode: ReplyMode,
    closure: impl AsyncFnOnce(&Draft, &Tether),
) {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params();
    params.mail_settings.as_mut().unwrap().attach_public_key = true;

    ctx.setup_user(params.clone()).await;

    // Create one message we can reply to.
    let mut remote_existing_message = draft_message();

    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    remote_existing_message.body.attachments.reverse();

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    if pre_attach_public_key {
        let addresses = Address::find("ORDER BY display_order ASC LIMIT 1", vec![], &tether)
            .await
            .unwrap();

        let address = addresses.first().unwrap();

        let public_key = Attachment::gen_public_key(&user_ctx, address, &tether)
            .await
            .unwrap();

        remote_existing_message
            .body
            .attachments
            .push(MessageAttachment {
                id: AttachmentId::from("public-key-attachment"),
                disposition: Disposition::Attachment,
                enc_signature: None,
                headers: MessageAttachmentHeaders {
                    content_disposition: ContentDisposition::One("attachment".to_string()),
                    content_id: None,
                    content_transfer_encoding: None,
                    image_height: None,
                    image_width: None,
                },
                key_packets: KeyPackets::from_vec(vec![]),
                mime_type: "application/pgp-keys".to_string(),
                name: public_key.attachment.filename,
                signature: None,
                size: 0,
            })
    }

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

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

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    // Decrypted message downloads attachments.
    for attachment in &message.body.attachments {
        ctx.mock_maybe_get_attachment_data(attachment.id.clone(), vec![])
            .await;
    }

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Insert attachment data into the cache.
            for attachment in &remote_existing_message.body.attachments {
                let local_attachment_id =
                    Attachment::remote_id_counterpart(attachment.id.clone(), tx)
                        .await
                        .unwrap()
                        .unwrap();

                Attachment::store_in_cache(
                    &user_ctx,
                    &attachment.name,
                    local_attachment_id,
                    attachment.name.as_bytes().to_vec(),
                    tx,
                )
                .await
                .unwrap();
            }
            Ok(())
        })
        .await
        .unwrap();

    // Create draft.
    let draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        reply_mode,
        true,
    )
    .await
    .unwrap();

    closure(&draft, &tether).await;
}

#[tokio::test]
async fn replying_to_expiring_message_inherits_expiration() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params_with_mime_type(MimeType::TextHtml);
    params.addresses[0].status = AddressStatus::Disabled;

    // Create one message we can reply to.
    let mut remote_existing_message = draft_message_with_attachments();

    let time = UnixTimestamp::now().saturating_add(100);
    let expiration_time = UnixTimestamp::now().saturating_add(2000);

    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.body.reply_to.address = "me@proton.me".into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    remote_existing_message.metadata.expiration_time = expiration_time.as_u64();
    remote_existing_message.metadata.time = time.as_u64();

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();

    let expected_expiration_time = UnixTimestamp::now()
        .saturating_add(expiration_time.saturating_sub(time.as_u64()).as_u64())
        .to_date_time()
        .unwrap();

    // Create draft.
    let draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        ReplyMode::Sender,
        true,
    )
    .await
    .unwrap();

    let DraftExpirationTime::Custom(expiration_time) = draft.expiration_time().await.unwrap()
    else {
        unreachable!()
    };

    // Due to this method relying on Unixtimestamp::now(), it should be higher or equal
    assert!(expiration_time >= expected_expiration_time);
}

#[tokio::test]
async fn draft_save_handles_save_with_new_message_if_remote_message_already_exists_after_server_response()
 {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params_with_mime_type(MimeType::TextHtml);
    params.addresses[0].status = AddressStatus::Disabled;

    // Create one message we can reply to.
    let mut remote_existing_message = draft_message_with_attachments();

    let time = UnixTimestamp::now().saturating_add(100);
    let expiration_time = UnixTimestamp::now().saturating_add(2000);

    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.body.reply_to.address = "me@proton.me".into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    remote_existing_message.metadata.expiration_time = expiration_time.as_u64();
    remote_existing_message.metadata.time = time.as_u64();
    remote_existing_message
        .metadata
        .attachments_metadata
        .clear();
    remote_existing_message.body.attachments.clear();

    let params = draft_test_params();
    let message = draft_message();

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    let expected_draft_params = expected_create_reply_draft_params(
        &existing_message,
        MimeType::TextHtml,
        ReplyMode::Sender,
    );

    ctx.mock_create_draft(
        expected_draft_params,
        None,
        message.clone(),
        Some(remote_existing_message.metadata.id.clone()),
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();
    // Create draft.
    let draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        ReplyMode::Sender,
        true,
    )
    .await
    .unwrap();

    draft.save().await.unwrap();

    // Crate the message to simulate some other rogue event/state update creating the
    // new draft before the server has a chance to respond. We also setup the conversation remote
    // id to simulate a reply scenario.

    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let mut new_message = Message::from_api_metadata(message.metadata.clone(), &tether)
        .await
        .unwrap();
    tether
        .tx(async |tx| new_message.save(tx).await)
        .await
        .unwrap();

    let draft_message_id = draft.message_id().await.unwrap().unwrap();
    assert_ne!(draft_message_id, new_message.id());
    user_ctx.execute_all_send_actions().await.unwrap();
}

#[tokio::test]
async fn draft_reply_handles_conversation_id_change_on_new_subject() {
    // changing the subject of a message in a reply can lead to reply with
    // a new conversation id that is not known yet.
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params_with_mime_type(MimeType::TextHtml);
    params.addresses[0].status = AddressStatus::Disabled;

    // Create one message we can reply to.
    let mut remote_existing_message = draft_message_with_attachments();

    let time = UnixTimestamp::now().saturating_add(100);
    let expiration_time = UnixTimestamp::now().saturating_add(2000);

    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.body.reply_to.address = "me@proton.me".into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    remote_existing_message.metadata.expiration_time = expiration_time.as_u64();
    remote_existing_message.metadata.time = time.as_u64();
    remote_existing_message
        .metadata
        .attachments_metadata
        .clear();
    remote_existing_message.body.attachments.clear();

    let params = draft_test_params();
    let message = draft_message();

    let changed_conversation_id = ConversationId::new("surprise!".into());

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    let expected_draft_params = expected_create_reply_draft_params(
        &existing_message,
        MimeType::TextHtml,
        ReplyMode::Sender,
    );

    let message_with_new_conv_id = {
        let mut m = message.clone();
        m.metadata.conversation_id = changed_conversation_id.clone();
        m
    };

    ctx.mock_create_draft(
        expected_draft_params,
        None,
        message_with_new_conv_id,
        Some(remote_existing_message.metadata.id.clone()),
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();
    // Create draft.
    let draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        ReplyMode::Sender,
        true,
    )
    .await
    .unwrap();

    draft.save().await.unwrap();

    let first_conversation_id = draft.conversation_id().await.unwrap().unwrap();
    user_ctx.execute_all_send_actions().await.unwrap();
    let local_changed_conversation_id =
        Conversation::remote_id_counterpart(changed_conversation_id.clone(), &tether)
            .await
            .unwrap()
            .unwrap();
    let second_conversation_id = draft.conversation_id().await.unwrap().unwrap();
    assert_ne!(first_conversation_id, second_conversation_id);
    assert_eq!(local_changed_conversation_id, second_conversation_id)
}

#[tokio::test]
async fn draft_reply_handles_conversation_id_change_multiple_times_without_destroying_existing_conversations()
 {
    // changing the subject of a message in a reply can lead to reply with
    // a new conversation id that is not known yet. This can happen multiple times
    // over some updates.
    // There was a bug that would delete existing conversation code and invalidate the local parent
    // id in the draft metadata, leading to a foreing constraint error.
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params_with_mime_type(MimeType::TextHtml);
    params.addresses[0].status = AddressStatus::Disabled;

    // Create one message we can reply to.
    let mut remote_existing_message = draft_message_with_attachments();

    let time = UnixTimestamp::now().saturating_add(100);
    let expiration_time = UnixTimestamp::now().saturating_add(2000);

    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.body.reply_to.address = "me@proton.me".into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    remote_existing_message.metadata.expiration_time = expiration_time.as_u64();
    remote_existing_message.metadata.time = time.as_u64();
    remote_existing_message
        .metadata
        .attachments_metadata
        .clear();
    remote_existing_message.body.attachments.clear();

    let params = draft_test_params();
    let message = draft_message();

    let changed_conversation_id = ConversationId::new("surprise!".into());

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    let expected_draft_params = expected_create_reply_draft_params(
        &existing_message,
        MimeType::TextHtml,
        ReplyMode::Sender,
    );

    let message_with_new_conv_id = {
        let mut m = message.clone();
        m.metadata.conversation_id = changed_conversation_id.clone();
        m
    };

    let mut conv = Conversation {
        remote_id: Some(changed_conversation_id.clone()),
        ..Conversation::test_default()
    };

    user_ctx
        .user_stash()
        .connection()
        .await
        .unwrap()
        .tx(async |tx| conv.save(tx).await)
        .await
        .unwrap();

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        Some(remote_existing_message.metadata.id.clone()),
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    ctx.mock_update_draft(
        message_with_new_conv_id.metadata.id.clone(),
        expected_draft_params.clone(),
        message_with_new_conv_id,
        DraftAttachmentKeyPackets::new(),
    )
    .await;

    ctx.mock_update_draft(
        message.metadata.id.clone(),
        {
            let mut params = expected_draft_params.clone();
            params.subject = "foo12345".into();
            params
        },
        message.clone(),
        DraftAttachmentKeyPackets::new(),
    )
    .await;

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();
    // Create draft.
    let draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        ReplyMode::Sender,
        true,
    )
    .await
    .unwrap();

    draft.save().await.unwrap();

    user_ctx.execute_all_send_actions().await.unwrap();

    // update once, changes conv id
    draft.save().await.unwrap();
    user_ctx.execute_all_send_actions().await.unwrap();

    // update again, changes conv id again
    draft.set_subject("foo12345".into()).await.unwrap();
    draft.save().await.unwrap();
    user_ctx.execute_all_send_actions().await.unwrap();
}

#[tokio::test]
async fn draft_reply_handles_conversation_id_change_on_new_subject_from_place_holder_to_existing_conversation()
 {
    // ch anging the subject of a message in a reply can lead to reply with
    // a new conversation id that is not known yet. It's also possible for to enter
    // a case where the converastion is swapped to a conversation we already have and
    // backend then deletes the placeholder, resulting in loss of the drat metadata.

    // We also need to make sure that attachments are preserved.
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params_with_mime_type(MimeType::TextHtml);
    params.addresses[0].status = AddressStatus::Disabled;
    // Create one message we can reply to.
    let mut remote_existing_message = draft_message_with_attachments();

    let time = UnixTimestamp::now().saturating_add(100);
    let expiration_time = UnixTimestamp::now().saturating_add(2000);

    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.body.reply_to.address = "me@proton.me".into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    remote_existing_message.metadata.expiration_time = expiration_time.as_u64();
    remote_existing_message.metadata.time = time.as_u64();
    remote_existing_message
        .metadata
        .attachments_metadata
        .clear();
    remote_existing_message.body.attachments.clear();

    let pub_key_attachment_id = AttachmentId::from("PUB_KEY");
    let pub_key_filename = "publickey - rust_test@proton.ch - 0x11B13868.asc";

    let mut params = draft_test_params();
    let mut mail_settings = params.mail_settings.unwrap_or_default();
    mail_settings.attach_public_key = true;
    params.mail_settings = Some(mail_settings);
    let message = draft_message();

    let changed_conversation_id = ConversationId::new("surprise!".into());

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let existing_converation_remote_id = ConversationId::from("already_exists_conv");
    let mut existing_conversation = Conversation {
        remote_id: Some(existing_converation_remote_id.clone()),
        has_messages: true,
        ..Conversation::test_default()
    };

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| {
            existing_conversation.save(tx).await?;
            existing_message.save(tx).await
        })
        .await
        .unwrap();

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    let expected_draft_params = expected_create_reply_draft_params(
        &existing_message,
        MimeType::TextHtml,
        ReplyMode::Sender,
    );

    let message_with_new_conv_id = {
        let mut m = message.clone();
        m.metadata.conversation_id = changed_conversation_id.clone();
        m
    };

    let attachment_key_packets = {
        let mut packets = DraftAttachmentKeyPackets::new();
        packets.insert(pub_key_attachment_id.clone(), KeyPackets::from_vec(vec![]));
        packets
    };

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message_with_new_conv_id.clone(),
        Some(remote_existing_message.metadata.id.clone()),
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    ctx.mock_create_attachment(
        NewAttachmentParams {
            filename: pub_key_filename.into(),
            message_id: message.metadata.id.clone(),
            mime_type: "application/pgp-keys".into(),
            disposition: NewAttachmentDisposition::Attachment,
            key_packets: vec![],
            signature: None,
            enc_signature: None,
            data_packet: vec![],
        },
        Ok(PostAttachmentResponse {
            attachment: NewAttachmentResponse {
                id: pub_key_attachment_id.clone(),
                disposition: Disposition::Attachment,
                enc_signature: None,
                key_packets: KeyPackets::from_vec(vec![]),
                signature: None,
                file_name: pub_key_filename.into(),
                file_size: 1024,
                headers: MessageAttachmentHeaders {
                    content_disposition: ContentDisposition::One("attachment".into()),
                    content_id: None,
                    content_transfer_encoding: None,
                    image_height: None,
                    image_width: None,
                },
            },
        }),
    )
    .await;

    ctx.mock_update_draft(
        message.metadata.id.clone(),
        expected_draft_params.clone(),
        message_with_new_conv_id.clone(),
        attachment_key_packets.clone(),
    )
    .await;

    ctx.mock_update_draft(
        message.metadata.id.clone(),
        {
            let mut params = expected_draft_params.clone();
            params.subject = "foo12345".into();
            params
        },
        {
            let mut message = message.clone();
            message.metadata.conversation_id = existing_converation_remote_id.clone();
            message
        },
        attachment_key_packets,
    )
    .await;

    // Get the message body - required to reply to draft.
    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();
    // Create draft.
    let draft = Draft::reply(
        &user_ctx,
        existing_message.local_id.unwrap(),
        ReplyMode::Sender,
        true,
    )
    .await
    .unwrap();

    assert!(
        Conversation::find_by_remote_id(changed_conversation_id.clone(), &tether)
            .await
            .unwrap()
            .is_none()
    );
    draft.save().await.unwrap();

    let first_conversation_id = draft.conversation_id().await.unwrap().unwrap();
    user_ctx.execute_all_send_actions().await.unwrap();
    let local_changed_conversation_id =
        Conversation::remote_id_counterpart(changed_conversation_id.clone(), &tether)
            .await
            .unwrap()
            .unwrap();
    let second_conversation_id = draft.conversation_id().await.unwrap().unwrap();
    assert_ne!(first_conversation_id, second_conversation_id);
    assert_eq!(local_changed_conversation_id, second_conversation_id);

    // simulate a converation update from api which introduces the attachment
    tether
        .tx(async |tx| {
            Conversation {
                remote_id: Some(changed_conversation_id.clone()),
                attachments_metadata: vec![AttachmentMetadata {
                    local_id: Some(
                        Attachment::remote_id_counterpart(pub_key_attachment_id.clone(), tx)
                            .await
                            .unwrap()
                            .unwrap(),
                    ),
                    attachment_type: AttachmentType::Remote(Some(pub_key_attachment_id.clone())),
                    disposition: Disposition::Attachment.into(),
                    mime_type: AttachmentMimeType::from_str("application/pgp-keys").unwrap(),
                    filename: pub_key_filename.into(),
                    size: 1024,
                }],
                ..Conversation::test_default()
            }
            .save(tx)
            .await
        })
        .await
        .unwrap();
    // change subject again to simulate another conv update.
    draft.set_subject("foo12345".into()).await.unwrap();
    draft.save().await.unwrap();

    user_ctx.execute_all_send_actions().await.unwrap();
    let third_conversation_id = draft.conversation_id().await.unwrap().unwrap();
    assert_ne!(first_conversation_id, third_conversation_id);
    assert_eq!(existing_conversation.id(), third_conversation_id);

    // Finally deleting the placeholder should not cause the draft metatadata to expire.
    tether
        .tx(async |tx| Conversation::delete_by_id(second_conversation_id, tx).await)
        .await
        .unwrap();

    assert!(
        DraftMetadata::find_by_id(draft.metadata_id, &tether)
            .await
            .unwrap()
            .is_some()
    );

    // Attachment should not be deleted by placedholder conversation deletion
    assert!(
        Attachment::find_by_remote_id(
            &AttachmentType::Remote(Some(pub_key_attachment_id.clone())),
            &tether
        )
        .await
        .unwrap()
        .is_some()
    );
    let attachments = DraftAttachmentMetadata::attachment_for_draft(draft.metadata_id, &tether)
        .await
        .unwrap();
    assert_eq!(attachments[0].remote_id().unwrap(), pub_key_attachment_id);
}
