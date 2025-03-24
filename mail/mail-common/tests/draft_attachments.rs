use crate::drafts_common::{draft_message, draft_test_params, expected_create_draft_params};
use proton_api_core::consts::Mail;
use proton_api_core::services::proton::UserId;
use proton_api_core::services::proton::common::ApiErrorInfo;
use proton_api_mail::services::proton::common::MessageId;
use proton_api_mail::services::proton::prelude::{
    AttachmentId, DraftAttachmentKeyPackets, MessageAttachmentHeaders, NewAttachmentDisposition,
    NewAttachmentResponse, PostAttachmentResponse,
};
use proton_api_mail::services::proton::request_data::NewAttachmentParams;
use proton_core_common::models::ModelExtension;
use proton_crypto_inbox::attachment::{
    BinaryAttachmentEncryptedSignature, BinaryAttachmentSignature, KeyPackets,
};
use proton_mail_common::MailUserContext;
use proton_mail_common::datatypes::Disposition;
use proton_mail_common::draft::attachments::DraftAttachmentState;
use proton_mail_common::draft::{Draft, DraftSyncStatus};
use proton_mail_common::models::Attachment;
use proton_mail_test_utils::message_body::{TEST_USER_ID, message_body_test_user_secret};
use proton_mail_test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::stash::Tether;
use std::path::Path;

mod drafts_common;

#[tokio::test]
async fn attachment_not_removed_on_error() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.mail_user_context().await;

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
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Create attachment
    let mut tether = user_ctx.user_stash().connection();
    let local_attachment =
        create_and_add_attachment(&user_ctx, attachment_file.path(), &mut draft, &mut tether).await;

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap_err();

    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();
    let draft_attachments = draft.attachments(&tether).await.unwrap();
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
    let draft_attachments = draft.attachments(&tether).await.unwrap();
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
    let user_ctx = ctx.mail_user_context().await;

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Create attachment
    let attachment_file = tempfile::NamedTempFile::new().unwrap();
    let mut tether = user_ctx.user_stash().connection();
    let attachment =
        create_and_add_attachment(&user_ctx, attachment_file.path(), &mut draft, &mut tether).await;

    let action_id = draft
        .remove_attachment(&user_ctx, attachment.local_id.unwrap())
        .await
        .unwrap();

    let draft_attachments = draft.attachments(&tether).await.unwrap();
    assert_eq!(draft_attachments.len(), 0);

    // cancelling the action  should undo the change.
    user_ctx.action_queue().cancel(action_id).await.unwrap();

    let draft_attachments = draft.attachments(&tether).await.unwrap();
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
    let user_ctx = ctx.mail_user_context().await;

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
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Create attachment
    let mut tether = user_ctx.user_stash().connection();
    let local_attachment =
        create_and_add_attachment(&user_ctx, attachment_file.path(), &mut draft, &mut tether).await;

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap_err();

    // Remove attachment
    draft
        .remove_attachment(&user_ctx, local_attachment.local_id.unwrap())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let draft_attachments = draft.attachments(&tether).await.unwrap();
    assert_eq!(draft_attachments.len(), 0);

    // Check attachment was deleted
    assert!(
        Attachment::find_by_id(local_attachment.local_id.unwrap(), &tether)
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
    let user_ctx = ctx.mail_user_context().await;

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
                    proton_api_mail::services::proton::response_data::Disposition::Attachment,
                key_packets: KeyPackets::from(""),
                signature: None,
                enc_signature: None,
                headers: MessageAttachmentHeaders {
                    content_disposition: "attachment".to_string(),
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
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Create attachment
    let mut tether = user_ctx.user_stash().connection();
    let local_attachment =
        create_and_add_attachment(&user_ctx, attachment_file.path(), &mut draft, &mut tether).await;

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Remove attachment
    draft
        .remove_attachment(&user_ctx, local_attachment.local_id.unwrap())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let draft_attachments = draft.attachments(&tether).await.unwrap();
    assert_eq!(draft_attachments.len(), 0);

    // Check attachment was deleted
    assert!(
        Attachment::find_by_id(local_attachment.local_id.unwrap(), &tether)
            .await
            .unwrap()
            .is_none()
    );
}

async fn create_and_add_attachment(
    ctx: &MailUserContext,
    path: &Path,
    draft: &mut Draft,
    tether: &mut Tether,
) -> Attachment {
    tokio::fs::write(path, "Hello World").await.unwrap();

    let local_attachment = Attachment::create_local(
        ctx,
        draft.address_id.clone(),
        Disposition::Attachment,
        path,
        tether,
    )
    .await
    .unwrap();

    draft
        .add_attachment(ctx, local_attachment.clone())
        .await
        .unwrap();

    local_attachment
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
