use super::drafts_common::*;
use proton_core_api::services::proton::{AddressId, UserId};
use proton_core_common::models::{Address, ModelIdExtension};
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::MimeType;
use proton_mail_common::actions::draft::AttachmentRemove;
use proton_mail_common::datatypes::{MessageFlags, ParsedHeaders};
use proton_mail_common::draft::{Draft, ReplyMode};
use proton_mail_common::models::{
    DraftAttachmentMetadata, DraftAttachmentUploadState, Message, MessageBody, MessageBodyMetadata,
};
use proton_mail_common::test_utils::message_body::{
    TEST_USER_ID, generate_new_api_address, message_body_test_user_secret,
};
use proton_mail_common::test_utils::test_context::MailTestContext;
use stash::orm::Model;
use std::collections::HashMap;

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
    new_address.signature = "Kind Regards New Address".into();

    let old_address = params.addresses.first().cloned().unwrap();
    params.addresses.push(new_address.clone());

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    // Extract first attachment key id
    let draft_attachment_metadata = DraftAttachmentMetadata::find_by_metadata_id(
        draft.metadata_id,
        &user_ctx.user_stash().connection().await.unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(draft_attachment_metadata.len(), 1);
    assert!(draft_attachment_metadata[0].is_public_key);
    let first_key_attachment_id = draft_attachment_metadata[0].local_attachment_id;

    draft
        .change_sender_address(new_address.email.clone())
        .await
        .unwrap();
    let state = draft.state().await.unwrap();
    assert_eq!(state.address_id, new_address.id);
    assert_eq!(state.sender, new_address.email);
    assert!(!state.body.contains(&old_address.signature));
    assert!(state.body.contains(&new_address.signature));

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
        &user_ctx.user_stash().connection().await.unwrap(),
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
        &user_ctx.user_stash().connection().await.unwrap(),
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

#[tokio::test]
async fn change_sender_address_with_alias() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let mut params = draft_test_params();
    params.mail_settings.as_mut().unwrap().draft_mime_type = MimeType::TextPlain;

    let mut new_address = generate_new_api_address(
        AddressId::from("My-New-Address"),
        "my-new-email@proton.ch",
        "new-email-key-id",
    );
    new_address.signature = "Kind Regards New Address".into();

    let old_address = params.addresses.first().cloned().unwrap();
    params.addresses.push(new_address.clone());
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create message and relevant data
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let old_address = Address::find_by_remote_id(old_address.id, &tether)
        .await
        .unwrap()
        .unwrap();

    let alias_address = "rust_test+ALIAS@proton.ch";

    let mut message = Message {
        local_id: None,
        remote_id: Some(MessageId::from("MESSAGE")),
        local_conversation_id: None,
        remote_conversation_id: Some(ConversationId::from("CONV")),
        local_address_id: old_address.id(),
        remote_address_id: old_address.remote_id.clone().unwrap(),
        attachments_metadata: vec![],
        cc_list: Default::default(),
        bcc_list: Default::default(),
        deleted: false,
        exclusive_location: None,
        expiration_time: Default::default(),
        external_id: None,
        flags: MessageFlags::RECEIVED,
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![],
        num_attachments: 0,
        display_order: 0,
        sender: Default::default(),
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
        mime_type: Default::default(),
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

            MessageBody::html("Hello world")
                .store(message.id(), tx)
                .await
        })
        .await
        .unwrap();

    // Create draft.
    let draft = Draft::reply(&user_ctx, message.id(), ReplyMode::Sender, true)
        .await
        .unwrap();

    let addresses = draft.sender_addresses().await.unwrap();
    assert_eq!(addresses[0].email, alias_address);
    // change to another address
    let old_body = draft.body().await.unwrap();
    draft
        .change_sender_address(new_address.email.clone())
        .await
        .unwrap();
    assert_ne!(old_body, draft.body().await.unwrap()); //signature changed

    // change back to alias - alias is still present in the address list
    let addresses = draft.sender_addresses().await.unwrap();
    assert_eq!(addresses[0].email, alias_address);
    let old_body = draft.body().await.unwrap();
    draft
        .change_sender_address(alias_address.to_owned())
        .await
        .unwrap();
    assert_ne!(old_body, draft.body().await.unwrap()); //signature changed
    assert_eq!(draft.sender().await.unwrap(), alias_address);

    // change to the same email without alias
    let old_body = draft.body().await.unwrap();
    draft
        .change_sender_address(old_address.email.clone())
        .await
        .unwrap();
    assert_eq!(old_body, draft.body().await.unwrap()); //signature does not change.
    assert_eq!(draft.sender().await.unwrap(), old_address.email);
}
