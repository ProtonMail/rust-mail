use super::drafts_common::{
    draft_message, draft_test_params, draft_test_params_with_mime_type,
    expected_create_draft_params, expected_create_reply_draft_params,
};
use proton_action_queue::queue::{ActionError, AsActionError, QueuedError};
use proton_core_api::consts::Mail;
use proton_core_api::services::proton::UserId;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_common::models::ModelExtension;
use proton_crypto_inbox::attachment::{
    BinaryAttachmentEncryptedSignature, BinaryAttachmentSignature, KeyPackets,
};
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::{
    AttachmentId, ContentDisposition, DraftAction, DraftAttachmentKeyPackets,
    MessageAttachmentHeaders, MessageFlags, NewAttachmentDisposition, NewAttachmentResponse,
    PostAttachmentResponse,
};
use proton_mail_api::services::proton::request_data::NewAttachmentParams;
use proton_mail_api::services::proton::response_data::MessageAttachment;
use proton_mail_common::datatypes::attachment::ContentId;
use proton_mail_common::datatypes::{Disposition, MimeType};
use proton_mail_common::draft::attachments::DraftAttachmentState;
use proton_mail_common::draft::{Draft, DraftSyncStatus, ReplyMode};
use proton_mail_common::models::{
    Attachment, DraftAttachmentMetadata, DraftAttachmentUploadState, Message,
};
use proton_mail_common::test_utils::message_body::{
    TEST_USER_ID, message_body_test_message_mime, message_body_test_user_secret,
};
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use proton_mail_common::{MailContextError, MailUserContext, draft};
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use std::path::Path;

#[tokio::test]
async fn attachment_not_removed_on_error() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
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

    ctx.mock_create_attachment(
        new_attachment_params(attachment_file.path(), message.metadata.id.clone()),
        Err((
            422,
            ApiErrorInfo {
                code: Mail::TooManyAttachments as u32,
                error: None,
                details: None,
            },
        )),
    )
    .await;

    // Draft open always loads message from remote.
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Create attachment
    let mut tether = user_ctx.user_stash().connection();

    let local_attachment = create_and_add_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Attachment,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap_err();

    let draft_message_id = draft.message_id().await.unwrap().unwrap();
    let draft_attachments = draft.attachments().await.unwrap();
    assert_eq!(draft_attachments.len(), 1);
    assert!(matches!(
        draft_attachments[0].state,
        DraftAttachmentState::Error(_)
    ));
    assert_eq!(
        draft_attachments[0].metadata,
        local_attachment.clone().into()
    );

    // Opening this draft will sync the data from the server, since the upload failed of the attachment
    // failed we need to preserve the failed attachments or they will get lost.
    let (draft, sync_status) = Draft::open(&user_ctx, draft_message_id).await.unwrap();
    let draft_attachments = draft.attachments().await.unwrap();
    assert!(matches!(sync_status, DraftSyncStatus::Synced));
    assert_eq!(draft_attachments.len(), 1);
    assert!(matches!(
        draft_attachments[0].state,
        DraftAttachmentState::Error(_)
    ));
    assert_eq!(draft_attachments[0].metadata, local_attachment.into());
}

#[tokio::test]
async fn remove_attachment_updates_attachment_list() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Create attachment
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
    let mut tether = user_ctx.user_stash().connection();

    let attachment = create_and_add_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Attachment,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    let action_id = draft.remove_attachment(attachment.id()).await.unwrap();

    let draft_attachments = draft.attachments().await.unwrap();
    assert_eq!(draft_attachments.len(), 0);

    // cancelling the action  should undo the change.
    user_ctx.action_queue().cancel(action_id).await.unwrap();

    let draft_attachments = draft.attachments().await.unwrap();
    assert_eq!(draft_attachments.len(), 1);
}

#[tokio::test]
async fn remove_attachment_by_cid() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Create attachment
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
    let mut tether = user_ctx.user_stash().connection();

    let attachment = create_and_add_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Inline,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    let mut attachment2 = create_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Inline,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    // Make the content id the same to check if our query is working correctly.
    attachment2.content_id = attachment.content_id.clone();
    tether
        .tx(async |tx| attachment2.save(tx).await)
        .await
        .unwrap();

    // Removing with unknown cid is an error
    let err = draft
        .remove_attachment_with_cid(ContentId::new())
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        MailContextError::Draft(draft::Error::AttachmentUpload(
            draft::AttachmentUploadError::AttachmentMetadataNotFoundCid(_)
        ))
    ));

    let action_id = draft
        .remove_attachment_with_cid(attachment.content_id.unwrap())
        .await
        .unwrap();

    let draft_attachments = draft.attachments().await.unwrap();
    assert_eq!(draft_attachments.len(), 0);

    // cancelling the action  should undo the change.
    user_ctx.action_queue().cancel(action_id).await.unwrap();

    let draft_attachments = draft.attachments().await.unwrap();
    assert_eq!(draft_attachments.len(), 1);
}

#[tokio::test]
async fn removing_non_uploaded_attachment() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
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

    ctx.mock_create_attachment(
        new_attachment_params(attachment_file.path(), message.metadata.id.clone()),
        Err((
            422,
            ApiErrorInfo {
                code: Mail::TooManyAttachments as u32,
                error: None,
                details: None,
            },
        )),
    )
    .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Create attachment
    let mut tether = user_ctx.user_stash().connection();

    let local_attachment = create_and_add_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Attachment,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap_err();

    // Remove attachment
    draft
        .remove_attachment(local_attachment.id())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let draft_attachments = draft.attachments().await.unwrap();
    assert_eq!(draft_attachments.len(), 0);

    // Check attachment was deleted
    assert!(
        Attachment::find_by_id(local_attachment.id(), &tether)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn removing_uploaded_attachment() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
    let message = draft_message();
    let expected_draft_params = expected_create_draft_params();
    let new_attachment_id = AttachmentId::from("REMOTE_ATTACHMENT");

    ctx.setup_user(params.clone()).await;

    ctx.mock_create_draft(
        expected_draft_params,
        None,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;

    ctx.mock_create_attachment(
        new_attachment_params(attachment_file.path(), message.metadata.id.clone()),
        Ok(PostAttachmentResponse {
            attachment: NewAttachmentResponse {
                id: new_attachment_id.clone(),
                file_name: "new_file_name".to_string(),
                file_size: 1024,
                disposition:
                    proton_mail_api::services::proton::response_data::Disposition::Attachment,
                key_packets: KeyPackets::from(""),
                signature: None,
                enc_signature: None,
                headers: MessageAttachmentHeaders {
                    content_disposition: ContentDisposition::One("attachment".to_string()),
                    content_id: None,
                    content_transfer_encoding: None,
                    image_height: None,
                    image_width: None,
                },
            },
        }),
    )
    .await;

    ctx.mock_delete_attachment(new_attachment_id).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Create attachment
    let mut tether = user_ctx.user_stash().connection();

    let local_attachment = create_and_add_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Attachment,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Remove attachment
    draft
        .remove_attachment(local_attachment.id())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let draft_attachments = draft.attachments().await.unwrap();
    assert_eq!(draft_attachments.len(), 0);

    // Check attachment was deleted
    assert!(
        Attachment::find_by_id(local_attachment.id(), &tether)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn draft_reply_or_forward_creates_new_attachments() {
    let mime_type = MimeType::TextPlain;
    let reply_mode = ReplyMode::Forward;

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params_with_mime_type(mime_type);

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_mime();
    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    remote_existing_message.body.attachments.reverse();

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    let expected_draft_params =
        expected_create_reply_draft_params(&existing_message, mime_type, reply_mode);

    let mut message = draft_message();

    message.body.attachments = remote_existing_message.body.attachments.clone();

    if reply_mode != ReplyMode::Forward {
        message.body.attachments.retain(|a| {
            a.disposition == proton_mail_api::services::proton::prelude::Disposition::Inline
        })
    }

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        Some(DraftAction::from(reply_mode)),
        message.clone(),
        Some(existing_message.remote_id.clone().unwrap()),
        DraftAttachmentKeyPackets::new(),
    )
    .await;

    // Get the message body - required to reply to draft.
    let (_, remote_body) = Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.clone().unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();

    let mut attachment_key_packets = DraftAttachmentKeyPackets::new();

    for attachment in &remote_body.metadata.attachments {
        let id = AttachmentId::from(uuid::Uuid::new_v4().to_string());
        let new_key_packets = KeyPackets::from(format!("{id}-key-packets"));

        attachment_key_packets.insert(id.clone(), new_key_packets.clone());

        ctx.mock_create_attachment(
            NewAttachmentParams {
                filename: attachment.filename.clone(),
                message_id: message.metadata.id.clone(),
                mime_type: attachment.mime_type.to_string(),
                disposition: match attachment.disposition {
                    Disposition::Attachment => NewAttachmentDisposition::Attachment,
                    Disposition::Inline => {
                        NewAttachmentDisposition::Inline(attachment.content_id.clone().unwrap().into_inner())
                    }
                },
                key_packets: vec![],
                signature: Some(BinaryAttachmentSignature::from(vec![])),
                enc_signature: Some(BinaryAttachmentEncryptedSignature::from(vec![])),
                data_packet: vec![],
            },
            Ok(PostAttachmentResponse {
                attachment: NewAttachmentResponse {
                    id:id.clone(),
                    file_name: attachment.filename.clone(),
                    file_size: attachment.size,
                    disposition: match attachment.disposition {
                        Disposition::Attachment => {
                            proton_mail_api::services::proton::response_data::Disposition::Attachment
                        }
                        Disposition::Inline => {
                            proton_mail_api::services::proton::response_data::Disposition::Inline
                        }
                    },
                    key_packets: new_key_packets.clone(),
                    signature: None,
                    enc_signature: None,
                    headers: MessageAttachmentHeaders {
                        content_disposition: ContentDisposition::One("disposition".to_string()),
                        content_id: None,
                        content_transfer_encoding: None,
                        image_height: None,
                        image_width: None,
                    },
                },
            }),
        )
        .await;

        message.body.attachments.push(MessageAttachment {
            id,
            disposition: proton_mail_api::services::proton::response_data::Disposition::Attachment,
            enc_signature: None,
            headers: MessageAttachmentHeaders {
                content_disposition: ContentDisposition::One("".to_string()),
                content_id: None,
                content_transfer_encoding: None,
                image_height: None,
                image_width: None,
            },
            key_packets: new_key_packets.clone(),
            mime_type: "plain/text".to_string(),
            name: attachment.filename.clone(),
            size: attachment.size,
            signature: None,
        });
    }

    ctx.mock_update_draft(
        message.metadata.id.clone(),
        expected_draft_params,
        message.clone(),
        attachment_key_packets,
    )
    .await;

    ctx.catch_all().await;

    // Create draft.
    let draft = Draft::reply(&user_ctx, existing_message.id(), reply_mode, true)
        .await
        .unwrap();

    draft.save().await.unwrap();

    // Execute actions.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Check attachment states.
    let attachments = DraftAttachmentMetadata::find_by_metadata_id(
        draft.metadata_id,
        &user_ctx.user_stash().connection(),
    )
    .await
    .unwrap();

    assert_eq!(attachments.len(), 3);

    for attachment in attachments {
        assert!(matches!(
            attachment.state(),
            DraftAttachmentUploadState::Uploaded
        ));
    }
}

#[tokio::test]
async fn deleting_draft_metadata_cleans_not_uploaded_attachments() {
    let mime_type = MimeType::TextHtml;
    let reply_mode = ReplyMode::Forward;

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params_with_mime_type(mime_type);

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_mime();

    remote_existing_message.metadata.sender.address = "me@proton.me".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;
    remote_existing_message.body.attachments.reverse();

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

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
        message.body.attachments.retain(|a| {
            a.disposition == proton_mail_api::services::proton::prelude::Disposition::Inline
        })
    }

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    ctx.catch_all().await;

    Message::force_sync_message_and_body(
        &user_ctx,
        existing_message.remote_id.clone().unwrap(),
        false,
        &mut tether,
    )
    .await
    .unwrap();

    // Create draft.
    let draft = Draft::reply(&user_ctx, existing_message.id(), reply_mode, true)
        .await
        .unwrap();

    // Get attachments
    let attachments = DraftAttachmentMetadata::find_by_metadata_id(
        draft.metadata_id,
        &user_ctx.user_stash().connection(),
    )
    .await
    .unwrap();

    assert_eq!(attachments.len(), 3);

    // delete draft.
    draft.discard().await.unwrap();

    // Execute actions.
    user_ctx.execute_all_send_actions().await.unwrap();

    for attachment in &attachments {
        assert!(
            Attachment::find_by_id(
                attachment.local_attachment_id,
                &user_ctx.user_stash().connection()
            )
            .await
            .unwrap()
            .is_none()
        );
    }
}

#[tokio::test]
async fn override_attachment_name() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let attachment_file = tempfile::NamedTempFile::new().unwrap();

    tokio::fs::write(attachment_file.path(), "Hello World")
        .await
        .unwrap();

    let filename_override = "OverriddenFileName.exe";
    let draft = Draft::empty(&user_ctx).await.unwrap();

    let local_attachment = Attachment::create_local(
        &user_ctx,
        draft.address_id().await.unwrap(),
        Disposition::Attachment,
        attachment_file.path(),
        Some(filename_override.to_owned()),
        &mut user_ctx.user_stash().connection(),
    )
    .await
    .unwrap();

    assert_eq!(local_attachment.filename, filename_override);
}

#[tokio::test]
async fn total_attachment_size_more_than_limit_local() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
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
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Create on very large attachment
    let mut tether = user_ctx.user_stash().connection();
    {
        let mut attachment = Attachment {
            local_id: None,
            attachment_type: Default::default(),
            local_address_id: None,
            remote_address_id: None,
            local_conversation_id: None,
            remote_conversation_id: None,
            local_message_id: None,
            remote_message_id: None,
            disposition: Default::default(),
            enc_signature: None,
            is_auto_forwardee: false,
            key_packets: None,
            mime_type: Default::default(),
            filename: "".to_string(),
            sender: None,
            signature: None,
            size: Attachment::MAX_ATTACHMENT_SIZE,
            content_id: None,
            transfer_encoding: None,
            image_width: None,
            image_height: None,
        };
        tether
            .tx(async |tx| {
                attachment.save(tx).await?;
                let mut attachment_metadata =
                    DraftAttachmentMetadata::new(draft.metadata_id, attachment.id(), 1, false);
                attachment_metadata.save(tx).await
            })
            .await
            .unwrap();
    }

    // Create attachment
    let local_attachment = create_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Attachment,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    let err = draft.add_attachment(&local_attachment).await.unwrap_err();
    assert!(matches!(
        err,
        MailContextError::Draft(draft::Error::AttachmentUpload(
            draft::AttachmentUploadError::TotalAttachmentSizeTooLarge
        ))
    ));
}
#[tokio::test]
async fn total_attachment_size_more_than_limit() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
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

    ctx.mock_create_attachment(
        new_attachment_params(attachment_file.path(), message.metadata.id.clone()),
        Err((
            422,
            ApiErrorInfo {
                code: Mail::TooManyAttachments as u32,
                error: None,
                details: None,
            },
        )),
    )
    .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let mut tether = user_ctx.user_stash().connection();
    // Create normal attachment first so we don't trip the local check
    let _ = create_and_add_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Attachment,
        &mut draft,
        None,
        &mut tether,
    )
    .await;
    // Create on very large attachment
    {
        let mut attachment = Attachment {
            local_id: None,
            attachment_type: Default::default(),
            local_address_id: None,
            remote_address_id: None,
            local_conversation_id: None,
            remote_conversation_id: None,
            local_message_id: None,
            remote_message_id: None,
            disposition: Default::default(),
            enc_signature: None,
            is_auto_forwardee: false,
            key_packets: None,
            mime_type: Default::default(),
            filename: "".to_string(),
            sender: None,
            signature: None,
            size: Attachment::MAX_ATTACHMENT_SIZE,
            content_id: None,
            transfer_encoding: None,
            image_width: None,
            image_height: None,
        };
        tether
            .tx(async |tx| {
                attachment.save(tx).await?;
                let mut attachment_metadata =
                    DraftAttachmentMetadata::new(draft.metadata_id, attachment.id(), 1, false);
                attachment_metadata.save(tx).await
            })
            .await
            .unwrap();
    }

    // Execute action.
    let QueuedError::Action(err, _) = user_ctx.execute_all_send_actions().await.unwrap_err() else {
        unreachable!();
    };

    let err = err
        .as_action_error::<proton_mail_common::actions::draft::AttachmentUpload>()
        .unwrap();

    assert!(matches!(
        err,
        ActionError::Action(MailContextError::Draft(draft::Error::AttachmentUpload(
            draft::AttachmentUploadError::TotalAttachmentSizeTooLarge
        )))
    ));
}

#[tokio::test]
async fn total_attachment_count_exceeds_limit() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
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

    ctx.mock_create_attachment(
        new_attachment_params(attachment_file.path(), message.metadata.id.clone()),
        Err((
            422,
            ApiErrorInfo {
                code: Mail::TooManyAttachments as u32,
                error: None,
                details: None,
            },
        )),
    )
    .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let mut tether = user_ctx.user_stash().connection();

    // Create attachment before filling the database to avoid triggering local check

    let _ = create_and_add_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Attachment,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    // Create 100 small attachments
    {
        tether
            .tx::<_, _, StashError>(async |tx| {
                for _ in 0..100 {
                    let mut attachment = Attachment {
                        local_id: None,
                        attachment_type: Default::default(),
                        local_address_id: None,
                        remote_address_id: None,
                        local_conversation_id: None,
                        remote_conversation_id: None,
                        local_message_id: None,
                        remote_message_id: None,
                        disposition: Default::default(),
                        enc_signature: None,
                        is_auto_forwardee: false,
                        key_packets: None,
                        mime_type: Default::default(),
                        filename: "".to_string(),
                        sender: None,
                        signature: None,
                        size: 10,
                        content_id: None,
                        transfer_encoding: None,
                        image_width: None,
                        image_height: None,
                    };
                    attachment.save(tx).await?;
                    let mut attachment_metadata =
                        DraftAttachmentMetadata::new(draft.metadata_id, attachment.id(), 1, false);
                    attachment_metadata.save(tx).await?;
                }
                Ok(())
            })
            .await
            .unwrap();
    }

    // Execute action.
    let QueuedError::Action(err, _) = user_ctx.execute_all_send_actions().await.unwrap_err() else {
        unreachable!();
    };

    let err = err
        .as_action_error::<proton_mail_common::actions::draft::AttachmentUpload>()
        .unwrap();

    assert!(matches!(
        err,
        ActionError::Action(MailContextError::Draft(draft::Error::AttachmentUpload(
            draft::AttachmentUploadError::TooManyAttachments
        )))
    ));
}

#[tokio::test]
async fn total_attachment_count_exceeds_limit_local() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
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
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let mut tether = user_ctx.user_stash().connection();

    // Create 100 small attachments
    {
        tether
            .tx::<_, _, StashError>(async |tx| {
                for _ in 0..100 {
                    let mut attachment = Attachment {
                        local_id: None,
                        attachment_type: Default::default(),
                        local_address_id: None,
                        remote_address_id: None,
                        local_conversation_id: None,
                        remote_conversation_id: None,
                        local_message_id: None,
                        remote_message_id: None,
                        disposition: Default::default(),
                        enc_signature: None,
                        is_auto_forwardee: false,
                        key_packets: None,
                        mime_type: Default::default(),
                        filename: "".to_string(),
                        sender: None,
                        signature: None,
                        size: 10,
                        content_id: None,
                        transfer_encoding: None,
                        image_width: None,
                        image_height: None,
                    };
                    attachment.save(tx).await?;
                    let mut attachment_metadata =
                        DraftAttachmentMetadata::new(draft.metadata_id, attachment.id(), 1, false);
                    attachment_metadata.save(tx).await?;
                }
                Ok(())
            })
            .await
            .unwrap();
    }
    let local_attachment = create_attachment(
        &user_ctx,
        attachment_file.path(),
        Disposition::Attachment,
        &mut draft,
        None,
        &mut tether,
    )
    .await;

    let err = draft.add_attachment(&local_attachment).await.unwrap_err();
    assert!(matches!(
        err,
        MailContextError::Draft(draft::Error::AttachmentUpload(
            draft::AttachmentUploadError::TooManyAttachments
        ))
    ));
}

async fn create_and_add_attachment(
    ctx: &MailUserContext,
    path: &Path,
    disposition: Disposition,
    draft: &mut Draft,
    file_name_override: Option<String>,
    tether: &mut Tether,
) -> Attachment {
    let local_attachment =
        create_attachment(ctx, path, disposition, draft, file_name_override, tether).await;

    draft.add_attachment(&local_attachment).await.unwrap();

    local_attachment
}

async fn create_attachment(
    ctx: &MailUserContext,
    path: &Path,
    disposition: Disposition,
    draft: &mut Draft,
    file_name_override: Option<String>,
    tether: &mut Tether,
) -> Attachment {
    tokio::fs::write(path, "Hello World").await.unwrap();

    Attachment::create_local(
        ctx,
        draft.address_id().await.unwrap(),
        disposition,
        path,
        file_name_override,
        tether,
    )
    .await
    .unwrap()
}

fn new_attachment_params(file_path: &Path, message_id: MessageId) -> NewAttachmentParams {
    NewAttachmentParams {
        filename: file_path.file_name().unwrap().to_str().unwrap().to_owned(),
        message_id,
        mime_type: "text/plain".into(),
        disposition: NewAttachmentDisposition::Attachment,
        // these parameters are not checked and can be empty.
        key_packets: vec![],
        signature: Some(BinaryAttachmentSignature::from(vec![])),
        enc_signature: Some(BinaryAttachmentEncryptedSignature::from(vec![])),
        data_packet: vec![],
    }
}
