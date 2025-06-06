mod drafts_common;

use drafts_common::*;
use proton_core_api::services::proton::{AddressId, UserId};
use proton_mail_api::services::proton::prelude::MimeType;
use proton_mail_common::actions::draft::AttachmentRemove;
use proton_mail_common::draft::Draft;
use proton_mail_common::models::{DraftAttachmentMetadata, DraftAttachmentUploadState};
use proton_mail_common::test_utils::message_body::{
    TEST_USER_ID, generate_new_api_address, message_body_test_user_secret,
};
use proton_mail_common::test_utils::test_context::MailTestContext;

#[tokio::test]
async fn change_sender_address() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let mut params = draft_test_params();
    params.mail_settings.as_mut().unwrap().attach_public_key = true;
    params.mail_settings.as_mut().unwrap().draft_mime_type = MimeType::TextPlain;

    let mut new_address = generate_new_api_address(
        AddressId::from("My-New-Address"),
        "my-new-email@proton.ch",
        "new-email-key-id",
    );
    new_address.signature = "Kind Regards\n New Address".into();

    let old_address = params.addresses.first().cloned().unwrap();
    params.addresses.push(new_address.clone());

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(&user_ctx).await.unwrap();

    // Extract first attachment key id
    let draft_attachment_metadata = DraftAttachmentMetadata::find_by_metadata_id(
        draft.metadata_id,
        &user_ctx.user_stash().connection(),
    )
    .await
    .unwrap();
    assert_eq!(draft_attachment_metadata.len(), 1);
    assert!(draft_attachment_metadata[0].is_public_key);
    let first_key_attachment_id = draft_attachment_metadata[0].local_attachment_id;

    draft
        .change_sender_address_by_id(&user_ctx, new_address.id.clone())
        .await
        .unwrap();
    assert_eq!(draft.address_id, new_address.id);
    assert_eq!(draft.sender, new_address.email);
    dbg!(draft.body());
    assert!(!draft.body().contains(&old_address.signature));
    assert!(draft.body().contains(&new_address.signature));

    // One attachment remove action should be queued.
    assert_eq!(
        1,
        user_ctx
            .action_queue()
            .typed_actions_count::<AttachmentRemove>()
            .await
            .unwrap()
    );

    let draft_attachment_metadata = DraftAttachmentMetadata::find_by_metadata_id(
        draft.metadata_id,
        &user_ctx.user_stash().connection(),
    )
    .await
    .unwrap();

    // Two attachment metadata entries should be present, one for the previous address key and
    // the new key.
    assert_eq!(draft_attachment_metadata.len(), 2);
    assert!(draft_attachment_metadata[0].is_public_key);
    assert!(draft_attachment_metadata[1].is_public_key);

    // The original public key should be deleted
    // The new public key should be pending upload
    for metadata in draft_attachment_metadata {
        if metadata.local_attachment_id == first_key_attachment_id {
            assert!(metadata.deleted);
        } else {
            assert_eq!(metadata.state(), DraftAttachmentUploadState::Pending);
        }
    }

    let draft_attachments = DraftAttachmentMetadata::public_key_attachments(
        draft.metadata_id,
        &user_ctx.user_stash().connection(),
    )
    .await
    .unwrap();

    // Check that the attachment are actaully present.
    for attachment in draft_attachments {
        if attachment.local_id.unwrap() == first_key_attachment_id {
            assert!(attachment.filename.contains(&old_address.email))
        } else {
            assert!(attachment.filename.contains(&new_address.email))
        }
    }
}
