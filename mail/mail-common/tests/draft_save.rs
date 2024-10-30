use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_api_mail::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftParams, DraftRecipient, DraftSender,
};
use proton_api_mail::services::proton::response_data::{AttachmentMetadata, MessageFlags};
use proton_api_mail::services::proton::response_data::{
    Disposition, Message as ApiMessage, MessageAddress as ApiMessageAddress, MessageAttachment,
    MessageAttachmentHeaders,
};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_crypto_account::keys::{ArmoredPrivateKey, EncryptedKeyToken, KeyTokenSignature};
use proton_crypto_inbox::attachment::KeyPackets;
use proton_crypto_inbox::message::EncryptedDraft;
use proton_crypto_inbox::proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, KeyFlag, KeyId, LockedKey,
};
use proton_mail_common::datatypes::{MimeType, SystemLabelId};
use proton_mail_common::decrypted_message::DecryptedMessageBody;
use proton_mail_common::draft::{
    Draft, Error, ReplyMode, DEFAULT_SUBJECT, FORWARD_PREFIX, REPLY_PREFIX,
};
use proton_mail_common::models::{Attachment, Conversation, DraftMetadata, MailSettings, Message};
use proton_mail_common::MailContextError;
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::message_body::*;
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;

#[tokio::test]
async fn create_empty_draft() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.mail_user_context().await;

    let mut message = message_body_test_message_simple();
    message.metadata.label_ids.push(LabelId::drafts().into());

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params,
        DraftAction::Reply,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft.save(user_ctx.queue()).await.unwrap();

    // Execute action.
    user_ctx.execute_pending_actions().await.unwrap();

    // Load the draft.
    let draft_message_id = draft
        .message_id(user_ctx.user_stash())
        .await
        .unwrap()
        .unwrap();

    let draft_conversation_id = draft
        .conversation_id(user_ctx.user_stash())
        .await
        .unwrap()
        .unwrap();

    let draft_message = Message::load(draft_message_id, user_ctx.user_stash())
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(draft_message.remote_id, Some(message.metadata.id.into()));

    // Local conversation id should have been assigned.
    assert!(draft_message.local_conversation_id.is_some());
    assert_eq!(
        draft_conversation_id,
        draft_message.local_conversation_id.unwrap(),
    );

    // Check the draft has the draft label.
    assert!(draft_message.label_ids.contains(&LabelId::drafts()));

    // Loading the message body should not trigger any network requests.
    let message_body_metadata = Message::message_body(&user_ctx, draft_message.local_id.unwrap())
        .await
        .unwrap();

    assert!(message_body_metadata.metadata.attachments.is_empty());

    let conversation = Conversation::find_by_id(
        draft_message.local_conversation_id.unwrap(),
        user_ctx.user_stash(),
    )
    .await
    .unwrap()
    .unwrap();

    // Conversation remote id has been set.
    assert_eq!(
        conversation.remote_id.unwrap(),
        message.metadata.conversation_id.into()
    );
    // Conversation should also have the draft label.
    assert!(conversation
        .labels
        .iter()
        .find(|l| { l.remote_label_id == LabelId::drafts().into() })
        .is_some());

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
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.mail_user_context().await;

    let mut message = message_body_test_message_simple();
    message.metadata.label_ids.push(LabelId::drafts().into());

    let new_subject = "My New Subject";
    let new_body = "Hello world";
    let new_to_list = vec!["to@list.info".to_owned()];
    let new_cc_list = vec!["cc@list.info".to_owned()];
    let new_bcc_list = vec!["bcc@list.info".to_owned()];

    let mut updated_message = message.clone();
    updated_message.metadata.subject = new_subject.into();
    updated_message.metadata.to_list = new_to_list
        .iter()
        .cloned()
        .map(|v| ApiMessageAddress {
            address: v,
            ..Default::default()
        })
        .collect();
    updated_message.metadata.cc_list = new_cc_list
        .iter()
        .cloned()
        .map(|v| ApiMessageAddress {
            address: v,
            ..Default::default()
        })
        .collect();
    updated_message.metadata.bcc_list = new_bcc_list
        .iter()
        .cloned()
        .map(|v| ApiMessageAddress {
            address: v,
            ..Default::default()
        })
        .collect();

    let expected_draft_params = expected_create_draft_params();
    let expected_update_draft_params = {
        let mut params = expected_create_draft_params();
        params.subject = new_subject.to_owned();
        params.to_list = new_to_list
            .iter()
            .cloned()
            .map(|v| DraftRecipient {
                address: v,
                name: String::new(),
                group: None,
            })
            .collect();
        params.cc_list = new_cc_list
            .iter()
            .cloned()
            .map(|v| DraftRecipient {
                address: v,
                name: String::new(),
                group: None,
            })
            .collect();
        params.bcc_list = new_bcc_list
            .iter()
            .cloned()
            .map(|v| DraftRecipient {
                address: v,
                name: String::new(),
                group: None,
            })
            .collect();
        params
    };

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params,
        DraftAction::Reply,
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
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft.save(user_ctx.queue()).await.unwrap();

    // Execute action.
    user_ctx.execute_pending_actions().await.unwrap();

    // Update the draft
    draft.subject = new_subject.to_owned();
    draft.body = new_body.to_owned();
    draft.to_list = new_to_list.clone();
    draft.cc_list = new_cc_list.clone();
    draft.bcc_list = new_bcc_list.clone();
    draft.save(user_ctx.queue()).await.unwrap();
    user_ctx.execute_pending_actions().await.unwrap();

    let draft_message_id = draft
        .message_id(user_ctx.user_stash())
        .await
        .unwrap()
        .unwrap();

    // Opening the draft and check if all the information is up to date
    let draft = Draft::open(&user_ctx, draft_message_id).await.unwrap();
    assert_eq!(draft.body, new_body);
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
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.mail_user_context().await;

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_simple();
    remote_existing_message.metadata.id = "Fancy Remote Id".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message, user_ctx.user_stash())
            .await
            .unwrap();
    existing_message
        .save_using(user_ctx.user_stash())
        .await
        .unwrap();
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
        Err(MailContextError::Draft(Error::MessageBodyMissing(_)))
    ));
}

#[tokio::test]
async fn create_draft_reply_should_fail_for_drafts() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.mail_user_context().await;

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_simple();
    remote_existing_message.metadata.id = "Fancy Remote Id".into();
    // is draft checks whether received or sent flags are present
    // set to empty to consider it as a draft.
    remote_existing_message.metadata.flags = MessageFlags::empty();

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message, user_ctx.user_stash())
            .await
            .unwrap();
    existing_message
        .save_using(user_ctx.user_stash())
        .await
        .unwrap();
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
        Err(MailContextError::Draft(Error::ReplyOrForwardToDraft(_)))
    ));
}

#[tokio::test]
async fn metadata_is_create_for_existing_not_opened_draft() {
    // Simulate opening a draft that was created in another session. We should
    // create metadata for this message so the `Save` action works correctly.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.mail_user_context().await;

    let mut message = message_body_test_message_simple();
    message.metadata.label_ids.push(LabelId::drafts().into());

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message(&message.metadata.id, message.clone())
        .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mut message = Message::from_api_metadata(message.metadata, user_ctx.user_stash())
        .await
        .unwrap();

    // Save message.
    message.save_using(user_ctx.user_stash()).await.unwrap();

    assert!(
        DraftMetadata::find_by_message_id(message.local_id.unwrap(), user_ctx.user_stash())
            .await
            .unwrap()
            .is_none()
    );

    // Create draft.
    let draft = Draft::open(&user_ctx, message.local_id.unwrap())
        .await
        .unwrap();

    let draft_by_message_id =
        DraftMetadata::find_by_message_id(message.local_id.unwrap(), user_ctx.user_stash())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(draft.metadata_id, draft_by_message_id.id.unwrap());
    drop(draft);

    // Opening this draft again should not create new metadata;
    let draft = Draft::open(&user_ctx, message.local_id.unwrap())
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
async fn create_draft_forward_inherits_all_attachments() {
    let draft_body = create_draft_reply_impl(MimeType::TextPlain, ReplyMode::Forward).await;
    assert_eq!(draft_body.metadata.attachments.len(), 2);

    let attachment_1 = &draft_body.metadata.attachments[1];
    let attachment_2 = &draft_body.metadata.attachments[0];

    let inline_attachment = gen_inline_attachment();
    let normal_attachment = gen_normal_attachment();

    compare_inline_attachment(&attachment_1, inline_attachment);
    assert_eq!(
        attachment_2.remote_id.clone().unwrap(),
        normal_attachment.id.into()
    );
    assert_eq!(
        attachment_2.disposition,
        normal_attachment.disposition.into()
    );
    assert_eq!(attachment_2.filename, normal_attachment.name);
    assert_eq!(attachment_2.size, normal_attachment.size);
}

fn compare_inline_attachment(attachment: &Attachment, inline_attachment: MessageAttachment) {
    assert_eq!(
        attachment.remote_id.clone().unwrap(),
        inline_attachment.id.into()
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
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params_with_mime_type(mime_type);
    let user_ctx = ctx.mail_user_context().await;

    // Create one message we can reply to.
    let mut remote_existing_message = draft_message_with_attachments();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), user_ctx.user_stash())
            .await
            .unwrap();
    existing_message
        .save_using(user_ctx.user_stash())
        .await
        .unwrap();
    let existing_message = existing_message;

    let expected_draft_params =
        expected_create_reply_draft_params(&existing_message, mime_type, reply_mode);
    let mut message = message_body_test_message_simple();
    message.body.attachments = remote_existing_message.body.attachments.clone();
    if reply_mode != ReplyMode::Forward {
        message
            .body
            .attachments
            .retain(|a| a.disposition == Disposition::Inline)
    }
    message.metadata.label_ids.push(LabelId::drafts().into());

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
        DraftAction::from(reply_mode),
        message.clone(),
        Some(existing_message.remote_id.clone().unwrap().into()),
        key_packets,
    )
    .await;
    ctx.catch_all().await;

    // Get the message body - required to reply to draft.
    Message::message_body(&user_ctx, existing_message.local_id.unwrap())
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
    draft.save(user_ctx.queue()).await.unwrap();

    // Execute action.
    user_ctx.execute_pending_actions().await.unwrap();

    let draft_message_id = draft
        .message_id(user_ctx.user_stash())
        .await
        .unwrap()
        .unwrap();

    let draft_conversation_id = draft
        .conversation_id(user_ctx.user_stash())
        .await
        .unwrap()
        .unwrap();

    // Load the draft.
    let draft_message = Message::load(draft_message_id, user_ctx.user_stash())
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(draft_message.remote_id, Some(message.metadata.id.into()));

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

    let conversation = Conversation::find_by_id(
        draft_message.local_conversation_id.unwrap(),
        user_ctx.user_stash(),
    )
    .await
    .unwrap()
    .unwrap();

    // Conversation remote id has been set.
    assert_eq!(
        conversation.remote_id.unwrap(),
        message.metadata.conversation_id.into()
    );

    // Opening this draft should work;
    Draft::open(&user_ctx, draft_message_id).await.unwrap();

    draft_body
}

fn draft_test_params() -> TestParams {
    draft_test_params_impl(None)
}
fn draft_test_params_with_mime_type(mime_type: MimeType) -> TestParams {
    draft_test_params_impl(Some(mime_type))
}
fn draft_test_params_impl(mime_type: Option<MimeType>) -> TestParams {
    let mut mail_settings = message_body_test_mail_settings();
    if let Some(mime_type) = mime_type {
        mail_settings.draft_mime_type = mime_type.into();
    }
    let mut params = TestParams {
        user_info: Some(message_body_test_user_info()),
        addresses: message_body_test_addresses(),
        mail_settings: Some(mail_settings),
        ..Default::default()
    };

    // Add another address to check if the empty draft grabs the
    // correct primary address. Using this key will result in a crypto
    // error.
    params.addresses.push(ApiAddress {
        id: ApiRemoteId::from("GIBBERISH TEST ID"),
        email: "gibberish@proton.ch".to_owned(),
        send: true,
        receive: true,
        status: ApiAddressStatus::Enabled,
        domain_id: None,
        address_type: ApiAddressType::Original,
        order: 2,
        display_name: "gibberish".to_owned(),
        signature: "".to_owned(),
        keys: ApiAddressKeys(vec![LockedKey {
            id: KeyId::from("GIBBERISH"),
            version: 3,
            private_key: ArmoredPrivateKey::from("GIBBERISH".to_owned()),
            token: Some(EncryptedKeyToken::from("GIBBERISH".to_owned())),
            signature: Some(KeyTokenSignature::from("GIBBERISH".to_owned())),
            activation: None,
            primary: true,
            active: true,
            flags: Some(KeyFlag::from(3_u32)),
            recovery_secret: None,
            recovery_secret_signature: None,
            address_forwarding_id: None,
        }]),
        catch_all: false,
        proton_mx: true,
        signed_key_list: ApiAddressSignedKeyList {
            min_epoch_id: Some(3),
            max_epoch_id: Some(66),
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: Some("GIBBERISH".to_owned()),
            revision: 1,
        },
    });
    params
}

fn expected_create_draft_params() -> DraftParams {
    let address = message_body_test_addresses();
    DraftParams {
        subject: DEFAULT_SUBJECT.to_owned(),
        unread: false,
        sender: DraftSender {
            address: address[0].email.clone(),
            name: address[0].display_name.clone(),
        },
        to_list: vec![],
        cc_list: vec![],
        bcc_list: vec![],
        external_id: None,
        draft_flags: 0,
        body: EncryptedDraft(String::new()),
        mime_type: MailSettings::default().draft_mime_type.into(),
    }
}
fn expected_create_reply_draft_params(
    message: &Message,
    mime_type: MimeType,
    reply_mode: ReplyMode,
) -> DraftParams {
    let address = message_body_test_addresses();
    let mut params = DraftParams {
        subject: format!(
            "{} {}",
            if reply_mode == ReplyMode::Forward {
                FORWARD_PREFIX
            } else {
                REPLY_PREFIX
            },
            message.subject
        ),
        unread: false,
        sender: DraftSender {
            address: address[0].email.clone(),
            name: address[0].display_name.clone(),
        },
        to_list: vec![DraftRecipient {
            address: message.sender.address.clone(),
            name: message.sender.address.clone(),
            group: None,
        }],
        cc_list: vec![],
        bcc_list: vec![],
        external_id: None,
        draft_flags: 0,
        body: EncryptedDraft(String::new()),
        mime_type: mime_type.into(),
    };

    if reply_mode == ReplyMode::Forward {
        params.to_list.clear();
        params.cc_list.clear();
    }

    params
}

fn draft_message_with_attachments() -> ApiMessage {
    let mut remote_existing_message = message_body_test_message_simple();
    let normal_attchment = gen_normal_attachment();
    remote_existing_message.body.attachments =
        vec![gen_inline_attachment(), normal_attchment.clone()];

    remote_existing_message
        .metadata
        .attachments_metadata
        .push(AttachmentMetadata {
            id: normal_attchment.id,
            disposition: normal_attchment.disposition,
            mime_type: normal_attchment.mime_type,
            name: normal_attchment.name,
            size: normal_attchment.size,
        });

    remote_existing_message
}

fn gen_inline_attachment() -> MessageAttachment {
    MessageAttachment {
        id: "MyInlineAttachment".into(),
        disposition: Disposition::Inline,
        enc_signature: None,
        headers: MessageAttachmentHeaders {
            content_disposition: "inline".to_owned(),
            content_id: Some("InlineCID".to_owned()),
            content_transfer_encoding: None,
            image_height: None,
            image_width: None,
        },
        key_packets: KeyPackets::from("inline_key_packets"),
        mime_type: "image/jpeg".to_owned(),
        name: "image.jpeg".to_owned(),
        signature: None,
        size: 123,
    }
}
fn gen_normal_attachment() -> MessageAttachment {
    MessageAttachment {
        id: "MyAttachment".into(),
        disposition: Disposition::Attachment,
        enc_signature: None,
        headers: MessageAttachmentHeaders {
            content_disposition: "attachment".to_owned(),
            content_id: None,
            content_transfer_encoding: None,
            image_height: None,
            image_width: None,
        },
        key_packets: KeyPackets::from("key_packets"),
        mime_type: "application/pdf".to_owned(),
        name: "doc.pdf".to_owned(),
        signature: None,
        size: 1024,
    }
}
