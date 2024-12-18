use proton_action_queue::queue::{ActionError, AsActionError, QueuedError};
use proton_api_core::consts::CoreBundle;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType, ApiErrorInfo,
};
use proton_api_core::services::proton::responses::GetKeysAllResponse;
use proton_api_mail::services::proton::request_data::{
    DraftAction, DraftAttachmentKeyPackets, DraftParams, DraftRecipient, DraftSender,
};
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, ConversationLabel, MessageFlags, MessageRecipient,
};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_crypto_account::keys::{ArmoredPrivateKey, EncryptedKeyToken, KeyTokenSignature};
use proton_crypto_inbox::message::EncryptedDraft;
use proton_crypto_inbox::proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, KeyFlag, KeyId, LockedKey,
};
use proton_mail_common::datatypes::{MimeType, SystemLabelId};
use proton_mail_common::draft::compose::DEFAULT_SUBJECT;
use proton_mail_common::draft::recipients::{MaybeEmptyString, RecipientEntry};
use proton_mail_common::draft::Draft;
use proton_mail_common::models::{MailSettings, Message, MessageBodyMetadata};
use proton_mail_common::{draft, MailContextError};
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::message_body::*;
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;

#[tokio::test]
async fn basic_send_check() {
    // Check :
    // * Draft is saved before sent
    // * Send API endpoint is updated
    // * Draft is moved to sent folder
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.mail_user_context().await;

    let tether = user_ctx.user_stash().connection();

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });
    let mut sent_message = message.clone();
    message.metadata.label_ids.push(LabelId::drafts().into());
    sent_message.metadata.label_ids.push(LabelId::sent().into());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let sent_conversation = ApiConversation {
        id: message.metadata.conversation_id.clone(),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snooze_reminder: false,
        expiration_time: 0,
        labels: vec![ConversationLabel {
            id: LabelId::sent().into(),
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_messages: 1,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        }],
        num_attachments: 0,
        num_messages: 1,
        num_unread: 0,
        order: 0,
        recipients: vec![],
        senders: vec![],
        size: 0,
        subject: sent_message.metadata.subject.clone(),
    };

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params.clone(),
        DraftAction::Reply,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    ctx.mock_update_draft(
        message.metadata.id.clone(),
        expected_draft_params,
        message.clone(),
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    ctx.mock_send_draft_basic(
        message.metadata.id.clone(),
        sent_message.clone(),
        sent_conversation,
    )
    .await;
    ctx.core_test_context()
        .mock_get_keys_all(
            "foo@bar.com",
            GetKeysAllResponse {
                address_keys: Default::default(),
                catch_all_keys: None,
                is_proton: false,
                proton_mx: false,
                unverified_keys: None,
                warnings: vec![],
            },
        )
        .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();
    user_ctx
        .with_queue(|queue| draft.save(queue))
        .await
        .unwrap();

    // Save at least once so we can retrieve the message id.
    user_ctx.execute_pending_actions().await.unwrap();

    // get draft message id.
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    let save_action = draft.to_save_action();
    let send_action = draft.to_send_action().unwrap();

    user_ctx
        .with_queue(|queue| Draft::send(queue, save_action, send_action))
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_pending_actions().await.unwrap();
    let tether = user_ctx.user_stash().connection();
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert_eq!(draft_message.remote_id, Some(message.metadata.id.into()));
    assert!(draft_message.flags.contains(MessageFlags::SENT.into()));
    assert!(draft_message.label_ids.contains(&LabelId::sent()));
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));

    // Check body metadata was updated.
    let body_metadata = MessageBodyMetadata::for_message(draft_message_id, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body_metadata.header, sent_message.body.header);
}

#[tokio::test]
async fn send_fails_if_recipient_is_not_valid() {
    let err =
        send_fails_if_recipient_is_not_valid_impl(CoreBundle::KeyGetInputInvalid as u32).await;

    let err = err
        .as_action_error::<proton_mail_common::actions::draft::Send>()
        .unwrap();
    assert!(matches!(
        err,
        ActionError::Action(MailContextError::Draft(draft::Error::SendMessage(
            draft::PackageError::RecipientEmailInvalid(_)
        )))
    ));
}

#[tokio::test]
async fn send_fails_if_recipient_is_not_a_known_proton_address() {
    let err =
        send_fails_if_recipient_is_not_valid_impl(CoreBundle::KeyGetAddressMissing as u32).await;

    let err = err
        .as_action_error::<proton_mail_common::actions::draft::Send>()
        .unwrap();
    assert!(matches!(
        err,
        ActionError::Action(MailContextError::Draft(draft::Error::SendMessage(
            draft::PackageError::ProtonRecipientDoesNotExist(_)
        )))
    ));
}

async fn send_fails_if_recipient_is_not_valid_impl(api_error_code: u32) -> anyhow::Error {
    // Check :
    // * Draft is saved before sent
    // * Send API endpoint is updated
    // * Draft is moved to sent folder
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.mail_user_context().await;

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });
    let mut sent_message = message.clone();
    message.metadata.label_ids.push(LabelId::drafts().into());
    sent_message.metadata.label_ids.push(LabelId::sent().into());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params.clone(),
        DraftAction::Reply,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    ctx.core_test_context()
        .mock_get_keys_all_failure(
            "foo@bar.com",
            Some(false),
            ApiErrorInfo {
                code: api_error_code,
                error: None,
                details: None,
            },
        )
        .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();
    let save_action = draft.to_save_action();
    let send_action = draft.to_send_action().unwrap();

    user_ctx
        .with_queue(|queue| Draft::send(queue, save_action, send_action))
        .await
        .unwrap();

    // Execute action.
    let err = user_ctx.execute_pending_actions().await.unwrap_err();
    let MailContextError::QueuedAction(QueuedError::Action(err, _)) = err else {
        panic!("invalid error");
    };

    err
}

fn draft_test_params() -> TestParams {
    draft_test_params_impl(None)
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
        to_list: vec![DraftRecipient {
            address: "foo@bar.com".to_owned(),
            name: String::new(),
            group: None,
        }],
        cc_list: vec![],
        bcc_list: vec![],
        external_id: None,
        draft_flags: 0,
        body: EncryptedDraft(String::new()),
        mime_type: MailSettings::default().draft_mime_type.into(),
    }
}
