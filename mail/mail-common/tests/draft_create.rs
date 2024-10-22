mod common;

use common::init::Params as TestParams;
use common::message_body::*;
use common::TestContext;
use proton_action_queue::queue::ActionError;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_api_mail::services::proton::request_data::{
    DraftAction, DraftParams, DraftRecipient, DraftSender,
};
use proton_api_mail::services::proton::response_data::MessageFlags;
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_crypto_account::keys::{ArmoredPrivateKey, EncryptedKeyToken, KeyTokenSignature};
use proton_crypto_inbox::message::EncryptedDraft;
use proton_crypto_inbox::proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, KeyFlag, KeyId, LockedKey,
};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::draft::{Draft, Error, ReplyMode, DEFAULT_SUBJECT, REPLY_PREFIX};
use proton_mail_common::models::{Conversation, MailSettings, Message, NewDraftMetadata};
use proton_mail_common::MailContextError;
use stash::orm::Model;
#[tokio::test]
async fn create_empty_draft() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.user_context().await;

    let mut message = message_body_test_message_simple();
    message.metadata.label_ids.push(LabelId::drafts().into());

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params,
        DraftAction::Reply,
        message.clone(),
        None,
    )
    .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create draft.
    let draft_output = Draft::action_create_empty(user_ctx.queue()).await.unwrap();

    // Execute action.
    user_ctx.execute_pending_actions().await.unwrap();

    // Load the draft.
    let draft_message = Message::load(draft_output.local.message_id, user_ctx.user_stash())
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(draft_message.remote_id, Some(message.metadata.id.into()));

    // Local conversation id should still be the same.
    assert_eq!(
        draft_message.local_conversation_id.unwrap(),
        draft_output.local.conversation_id
    );

    // Check the draft has the draft label.
    assert!(draft_message.label_ids.contains(&LabelId::drafts().into()));

    // Loading the message body should not trigger any network requests.
    let _ = Message::message_body(&user_ctx, draft_message.local_id.unwrap())
        .await
        .unwrap();

    let conversation =
        Conversation::find_by_id(draft_output.local.conversation_id, user_ctx.user_stash())
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

    //TODO(ET-1361): Check body

    // Draft metadata should no longer exist.
    assert!(
        NewDraftMetadata::find_by_id(draft_message.local_id.unwrap(), user_ctx.user_stash())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn create_draft_reply_without_body_is_error() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.user_context().await;

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_simple();
    remote_existing_message.metadata.id = "Fancy Remote Id".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;

    let mut existing_message =
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
    let result = Draft::action_create_reply(
        user_ctx.queue(),
        ReplyMode::Sender,
        existing_message.local_id.unwrap(),
    )
    .await;

    assert!(matches!(
        result,
        Err(ActionError::Action(MailContextError::Draft(
            Error::MessageBodyMissing(_)
        )))
    ));
}

#[tokio::test]
async fn create_draft_reply_should_fail_for_drafts() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.user_context().await;

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_simple();
    remote_existing_message.metadata.id = "Fancy Remote Id".into();
    // is draft checks whether received or sent flags are present
    // set to empty to consider it as a draft.
    remote_existing_message.metadata.flags = MessageFlags::empty();

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;

    let mut existing_message =
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
    let result = Draft::action_create_reply(
        user_ctx.queue(),
        ReplyMode::Sender,
        existing_message.local_id.unwrap(),
    )
    .await;

    assert!(matches!(
        result,
        Err(ActionError::Action(MailContextError::Draft(
            Error::ReplyOrForwardToDraft(_)
        )))
    ));
}

#[tokio::test]
async fn create_draft_reply() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        RemoteId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let user_ctx = ctx.user_context().await;

    // Create one message we can reply to.
    let mut remote_existing_message = message_body_test_message_simple();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;

    let mut existing_message =
        Message::from_api_data(remote_existing_message.clone(), user_ctx.user_stash())
            .await
            .unwrap();
    existing_message
        .save_using(user_ctx.user_stash())
        .await
        .unwrap();
    let existing_message = existing_message;

    let expected_draft_params = expected_create_reply_draft_params(&existing_message);
    let mut message = message_body_test_message_simple();
    message.metadata.label_ids.push(LabelId::drafts().into());

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;
    ctx.mock_create_draft(
        expected_draft_params,
        DraftAction::Reply,
        message.clone(),
        Some(existing_message.remote_id.clone().unwrap().into()),
    )
    .await;
    ctx.catch_all().await;

    // Get the message body - required to reply to draft.
    Message::message_body(&user_ctx, existing_message.local_id.unwrap())
        .await
        .unwrap();

    // Create draft.
    let draft_output = Draft::action_create_reply(
        user_ctx.queue(),
        ReplyMode::Sender,
        existing_message.local_id.unwrap(),
    )
    .await
    .unwrap();

    // Execute action.
    user_ctx.execute_pending_actions().await.unwrap();

    // Load the draft.
    let draft_message = Message::load(draft_output.local.message_id, user_ctx.user_stash())
        .await
        .unwrap()
        .expect("failed to load message");
    assert_eq!(draft_message.remote_id, Some(message.metadata.id.into()));
    // Local conversation id should still be the same.
    assert_eq!(
        draft_message.local_conversation_id.unwrap(),
        draft_output.local.conversation_id
    );
    assert_eq!(
        draft_message.local_conversation_id.unwrap(),
        existing_message.local_conversation_id.unwrap(),
    );

    // Check the draft has the draft label.
    assert!(draft_message.label_ids.contains(&LabelId::drafts().into()));

    // Loading the message body should not trigger any network requests.
    let _ = Message::message_body(&user_ctx, draft_message.local_id.unwrap())
        .await
        .unwrap();

    let conversation =
        Conversation::find_by_id(draft_output.local.conversation_id, user_ctx.user_stash())
            .await
            .unwrap()
            .unwrap();

    // Conversation remote id has been set.
    assert_eq!(
        conversation.remote_id.unwrap(),
        message.metadata.conversation_id.into()
    );

    // Draft metadata should no longer exist.
    assert!(
        NewDraftMetadata::find_by_id(draft_message.local_id.unwrap(), user_ctx.user_stash())
            .await
            .unwrap()
            .is_none()
    );

    //TODO(ET-1361): Check body
}

fn draft_test_params() -> TestParams {
    let mut params = TestParams {
        user_info: Some(message_body_test_user_info()),
        addresses: message_body_test_addresses(),
        mail_settings: Some(message_body_test_mail_settings()),
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
fn expected_create_reply_draft_params(message: &Message) -> DraftParams {
    let address = message_body_test_addresses();
    DraftParams {
        subject: format!("{} {}", REPLY_PREFIX, message.subject),
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
        mime_type: MailSettings::default().draft_mime_type.into(),
    }
}
