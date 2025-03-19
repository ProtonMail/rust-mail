use crate::drafts_common::{draft_message, draft_test_params, expected_create_draft_params};
use proton_api_core::consts::Mail;
use proton_api_core::services::proton::common::ApiErrorInfo;
use proton_api_core::services::proton::UserId;
use proton_api_mail::services::proton::common::MessageId;
use proton_api_mail::services::proton::prelude::{
    DraftAttachmentKeyPackets, NewAttachmentDisposition,
};
use proton_api_mail::services::proton::request_data::NewAttachmentParams;
use proton_mail_common::datatypes::Disposition;
use proton_mail_common::draft::attachments::DraftAttachmentState;
use proton_mail_common::draft::{Draft, DraftSyncStatus};
use proton_mail_common::models::Attachment;
use proton_mail_test_utils::message_body::{message_body_test_user_secret, TEST_USER_ID};
use proton_mail_test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
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

    tokio::fs::write(attachment_file.path(), "Hello World")
        .await
        .unwrap();

    let local_attachment = Attachment::create_local(
        &user_ctx,
        draft.address_id.clone(),
        Disposition::Attachment,
        attachment_file.path(),
        &mut tether,
    )
    .await
    .unwrap();

    draft
        .add_attachment(&user_ctx, local_attachment.clone())
        .await
        .unwrap();

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
    let (draft, sync_status) = Draft::open(user_ctx.as_weak(), draft_message_id)
        .await
        .unwrap();
    let draft_attachments = draft.attachments(&tether).await.unwrap();
    assert!(matches!(sync_status, DraftSyncStatus::Synced));
    assert_eq!(draft_attachments.len(), 1);
    assert!(matches!(
        draft_attachments[0].state,
        DraftAttachmentState::Error(_)
    ));
    assert_eq!(draft_attachments[0].metadata, local_attachment.into());
}

fn new_attachment_params(file_path: &Path, message_id: MessageId) -> NewAttachmentParams {
    NewAttachmentParams {
        filename: file_path.file_name().unwrap().to_str().unwrap().to_owned(),
        message_id,
        mime_type: "text/plain".into(),
        disposition: NewAttachmentDisposition::Attachment,
        // these parameters are not checked and can be empty.
        key_packets: vec![],
        signature: None,
        enc_signature: None,
        data_packet: vec![],
    }
}
