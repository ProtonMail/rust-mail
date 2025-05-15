use chrono::{Days, Local, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use proton_action_queue::queue::{ActionError, AsActionError, QueuedError};
use proton_core_api::consts::{CoreBundle, Mail};
use proton_core_api::services::proton::GetKeysAllResponse;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressSignedKeyList as ApiAddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_core_api::services::proton::{AddressId, LabelId, UserId};
use proton_core_common::models::ModelExtension;
use proton_crypto_account::keys::{ArmoredPrivateKey, EncryptedKeyToken, KeyTokenSignature};
use proton_crypto_inbox::message::EncryptedDraft;
use proton_crypto_inbox::proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, KeyFlag, KeyId, LockedKey,
};
use proton_mail_api::services::proton::prelude::PostCancelSendResponse;
use proton_mail_api::services::proton::request_data::{
    DraftAttachmentKeyPackets, DraftParams, DraftRecipient, DraftSender,
};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationLabel, MessageFlags, MessageRecipient,
};
use proton_mail_common::datatypes::{MimeType, SystemLabelId};
use proton_mail_common::draft::Draft;
use proton_mail_common::draft::compose::DEFAULT_SUBJECT;
use proton_mail_common::draft::observers::DraftSendResultWatcher;
use proton_mail_common::draft::recipients::{MaybeEmptyString, RecipientEntry};
use proton_mail_common::models::{
    DraftSendFailure, DraftSendFailureSend, DraftSendResult, DraftSendResultOrigin, MailSettings,
    Message, MessageBodyMetadata,
};
use proton_mail_common::{MailContextError, MailUserContext, draft};
use proton_mail_ids::LocalMessageId;
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::message_body::*;
use proton_mail_test_utils::messages::TestDraftSendRequest;
use proton_mail_test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::stash::Bond;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn basic_send_check() {
    // Check :
    // * Draft is saved before sent
    // * Send API endpoint is updated
    // * Draft is moved to sent folder
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });
    let mut sent_message = message.clone();
    message.metadata.label_ids.push(LabelId::drafts());
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let sent_conversation = ApiConversation {
        id: message.metadata.conversation_id.clone(),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snooze_reminder: false,
        expiration_time: 0,
        labels: vec![ConversationLabel {
            id: LabelId::sent(),
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
        subject: sent_message.metadata.subject.clone(),
        ..Default::default()
    };

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
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
    ctx.mock_send_draft(
        message.metadata.id.clone(),
        default_mock_send_params(),
        sent_message.clone(),
        sent_conversation,
        (Utc::now().timestamp() + SEND_DELAY_SECONDS as i64).unsigned_abs(),
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
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Save at least once so we can retrieve the message id.
    user_ctx.execute_all_send_actions().await.unwrap();

    // get draft message id.
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    draft
        .send(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Check draft is in outbox.
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert!(draft_message.label_ids.contains(&LabelId::outbox()));
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(!draft_message.label_ids.contains(&LabelId::all_drafts()));

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();
    let tether = user_ctx.user_stash().connection();
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    // Check message is in the sent folder
    assert!(!draft_message.label_ids.contains(&LabelId::outbox()));
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(draft_message.label_ids.contains(&LabelId::sent()));

    assert_eq!(draft_message.remote_id, Some(message.metadata.id));
    assert!(draft_message.flags.contains(MessageFlags::SENT.into()));
    assert!(draft_message.label_ids.contains(&LabelId::sent()));
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));

    // Check body metadata was updated.
    let body_metadata = MessageBodyMetadata::for_message(draft_message_id, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body_metadata.header, sent_message.body.header);

    // Check send result was created.

    let send_result = DraftSendResult::find_by_id(draft_message_id, &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(send_result.is_success());
    assert_eq!(
        send_result.remote_message_id,
        Some(draft_message.remote_id.unwrap())
    );
    assert!(send_result.timestamp < send_result.undo_timestamp);
    assert!(!send_result.seen);
}

#[tokio::test]
async fn basic_schedule_send_check() {
    // Check :
    // * Draft is saved before sent
    // * Send API endpoint is updated
    // * Draft is moved to all_sent folder

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });
    let mut sent_message = message.clone();
    message.metadata.label_ids.push(LabelId::drafts());
    sent_message
        .metadata
        .label_ids
        .push(LabelId::all_scheduled());
    sent_message
        .metadata
        .flags
        .set(MessageFlags::SCHEDULED_SEND, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let sent_conversation = ApiConversation {
        id: message.metadata.conversation_id.clone(),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snooze_reminder: false,
        expiration_time: 0,
        labels: vec![ConversationLabel {
            id: LabelId::sent(),
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
        subject: sent_message.metadata.subject.clone(),
        ..Default::default()
    };

    let expected_draft_params = expected_create_draft_params();
    let delivery_time = Local::now().checked_add_months(Months::new(1)).unwrap();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
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
    ctx.mock_send_draft(
        message.metadata.id.clone(),
        default_mock_schedule_send_params(delivery_time.timestamp().unsigned_abs()),
        sent_message.clone(),
        sent_conversation,
        delivery_time.timestamp().unsigned_abs(),
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
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Save at least once so we can retrieve the message id.
    user_ctx.execute_all_send_actions().await.unwrap();

    // get draft message id.
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    draft
        .schedule_send(
            delivery_time,
            user_ctx.action_queue(),
            &user_ctx.user_stash().connection(),
        )
        .await
        .unwrap();

    // Check draft is in outbox.
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert!(draft_message.label_ids.contains(&LabelId::all_scheduled()));
    assert!(!draft_message.label_ids.contains(&LabelId::outbox()));
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(!draft_message.label_ids.contains(&LabelId::all_drafts()));
    // Time of the message should match the delivery time.
    assert_eq!(draft_message.time, delivery_time.timestamp().unsigned_abs());

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();
    let tether = user_ctx.user_stash().connection();
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    // Check message is in the sent folder
    assert!(!draft_message.label_ids.contains(&LabelId::outbox()));
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(draft_message.label_ids.contains(&LabelId::all_scheduled()));
    assert_eq!(draft_message.remote_id, Some(message.metadata.id));
    assert!(
        draft_message
            .flags
            .contains(MessageFlags::SCHEDULED_SEND.into())
    );

    // Check body metadata was updated.
    let body_metadata = MessageBodyMetadata::for_message(draft_message_id, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body_metadata.header, sent_message.body.header);

    // Check send result was created.

    let send_result = DraftSendResult::find_by_id(draft_message_id, &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(send_result.is_success());
    assert_eq!(
        send_result.remote_message_id,
        Some(draft_message.remote_id.unwrap())
    );
    assert!(send_result.timestamp < send_result.undo_timestamp);
    assert!(!send_result.seen);
}

#[tokio::test]
async fn schedule_send_with_old_delivery_time_fails() {
    // When schedule sending a message with a time in the past, we should fail.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });
    let mut sent_message = message.clone();
    message.metadata.label_ids.push(LabelId::drafts());
    sent_message
        .metadata
        .label_ids
        .push(LabelId::all_scheduled());
    sent_message
        .metadata
        .flags
        .set(MessageFlags::SCHEDULED_SEND, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let expected_draft_params = expected_create_draft_params();
    let delivery_time = Local::now().checked_sub_days(Days::new(2)).unwrap();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();

    draft
        .schedule_send(
            delivery_time,
            user_ctx.action_queue(),
            &user_ctx.user_stash().connection(),
        )
        .await
        .unwrap();

    let Err(QueuedError::Action(schedule_send_error, _)) =
        user_ctx.execute_all_send_actions().await
    else {
        unreachable!();
    };
    let schedule_send_error = schedule_send_error
        .as_action_error::<proton_mail_common::actions::draft::Send>()
        .unwrap();
    assert!(matches!(
        schedule_send_error,
        ActionError::Action(MailContextError::Draft(draft::Error::Send(
            draft::SendError::SechduleSendExpired,
        )))
    ));

    // get draft message id.
    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    // Check draft is back in the drafts folder
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert!(!draft_message.label_ids.contains(&LabelId::all_scheduled()));
    assert!(!draft_message.label_ids.contains(&LabelId::outbox()));
    assert!(draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(draft_message.label_ids.contains(&LabelId::all_drafts()));

    // Check send result was created.
    let send_result = DraftSendResult::find_by_id(draft_message_id, &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(!send_result.is_success());
    assert!(matches!(
        send_result.error,
        Some(DraftSendFailure::Send(
            DraftSendFailureSend::ScheduleSendExpired
        ))
    ));
}
#[tokio::test]
async fn send_fails_if_recipient_is_not_valid() {
    let (err, _, _) =
        send_fails_if_recipient_is_not_valid_impl(CoreBundle::KeyGetInputInvalid as u32).await;

    let err = err
        .as_action_error::<proton_mail_common::actions::draft::Send>()
        .unwrap();
    assert!(matches!(
        err,
        ActionError::Action(MailContextError::Draft(draft::Error::Send(
            draft::SendError::SendMessage(draft::PackageError::RecipientEmailInvalid(_))
        )))
    ));
}

#[tokio::test]
async fn send_fails_if_recipient_is_not_a_known_proton_address() {
    let (err, _, _) =
        send_fails_if_recipient_is_not_valid_impl(CoreBundle::KeyGetAddressMissing as u32).await;

    let err = err
        .as_action_error::<proton_mail_common::actions::draft::Send>()
        .unwrap();
    assert!(matches!(
        err,
        ActionError::Action(MailContextError::Draft(draft::Error::Send(
            draft::SendError::SendMessage(draft::PackageError::ProtonRecipientDoesNotExist(_))
        )))
    ));
}

#[tokio::test]
async fn send_fail_recorded_to_db() {
    let (_, local_id, ctx) =
        send_fails_if_recipient_is_not_valid_impl(CoreBundle::KeyGetInputInvalid as u32).await;

    let send_result = DraftSendResult::find_by_id(local_id, &ctx.user_stash().connection())
        .await
        .unwrap()
        .unwrap();
    assert!(!send_result.is_success());
    assert!(!send_result.seen);
    assert!(
        matches! { send_result.error, Some(DraftSendFailure::Send(DraftSendFailureSend::RecipientEmailInvalid(_)))}
    );
    assert_eq!(send_result.origin, DraftSendResultOrigin::Send);
}

#[tokio::test]
async fn send_fail_puts_message_back_in_drafts() {
    let (_, local_id, ctx) =
        send_fails_if_recipient_is_not_valid_impl(CoreBundle::KeyGetInputInvalid as u32).await;

    let send_result = DraftSendResult::find_by_id(local_id, &ctx.user_stash().connection())
        .await
        .unwrap()
        .unwrap();

    let draft_message =
        Message::find_by_id(send_result.local_message_id, &ctx.user_stash().connection())
            .await
            .unwrap()
            .unwrap();
    assert!(draft_message.label_ids.contains(&LabelId::drafts()));

    assert!(draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(!draft_message.label_ids.contains(&LabelId::outbox()));
    assert!(!draft_message.label_ids.contains(&LabelId::sent()));
}

#[tokio::test]
async fn draft_save_failure_creates_send_result_with_correct_origin_when_used_before_send() {
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
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".to_owned(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();
    draft
        .send(user_ctx.action_queue(), &user_ctx.user_stash().connection())
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
    assert_eq!(send_result.origin, DraftSendResultOrigin::SaveBeforeSend);
}

#[tokio::test]
async fn save_after_send_is_an_error() {
    // Re-saving a draft after a queued send action is not allowed.
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();
    draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Save at least once so we can retrieve the message id.
    draft
        .send(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    let result = draft
        .save(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await;
    let Err(e) = result else {
        panic!("Should have failed");
    };
    assert!(matches!(
        e,
        MailContextError::Draft(draft::Error::Save(draft::SaveError::AlreadySent))
    ));
}

#[tokio::test]
async fn already_sent_error_does_not_produce_error() {
    // Check :
    // * Draft is saved before sent
    // * Send API endpoint is updated
    // * Draft is moved to sent folder
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        None,
        DraftAttachmentKeyPackets::new(),
    )
    .await;
    ctx.mock_send_draft_failure(
        message.metadata.id.clone(),
        ApiErrorInfo {
            code: Mail::MessageAlreadySent as u32,
            error: None,
            details: None,
        },
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
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();
    draft
        .send(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    let mut observer = DraftSendResultWatcher::new(user_ctx.user_stash().clone())
        .await
        .unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(1), observer.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].is_success());
    // We have no send delivery time so we can't undo this.
    assert!(!result[0].is_send_undoable());
}

#[tokio::test]
async fn cancel_schedule_send_on_non_scheduled_message() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    let message = message_body_test_message_simple();
    let mut tether = user_ctx.user_stash().connection();
    let message = tether
        .tx::<_, _, MailContextError>(async |tx: &Bond<'_>| {
            let mut message = Message::from_api_metadata(message.metadata, tx).await?;
            message.save(tx).await?;
            Ok(message)
        })
        .await
        .unwrap();

    let err = Draft::cancel_schedule_send(&user_ctx, message.local_id.unwrap())
        .await
        .unwrap_err();
    matches!(
        err,
        MailContextError::Draft(draft::Error::CancelScheduleSend(
            draft::CancelScheduleSendError::MessageIsNotScheduled(_)
        ))
    );
}
#[tokio::test]
async fn cancel_schedule_send_on_queued_send() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });
    let mut sent_message = message.clone();
    message.metadata.label_ids.push(LabelId::drafts());
    sent_message
        .metadata
        .label_ids
        .push(LabelId::all_scheduled());
    sent_message
        .metadata
        .flags
        .set(MessageFlags::SCHEDULED_SEND, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let delivery_time = Local::now().checked_add_months(Months::new(1)).unwrap();

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();
    draft
        .schedule_send(
            delivery_time,
            user_ctx.action_queue(),
            &user_ctx.user_stash().connection(),
        )
        .await
        .unwrap();

    let draft_message_id = draft.message_id(&tether).await.unwrap().unwrap();

    // Cancels the queued action, no network requests are made.
    Draft::cancel_schedule_send(&user_ctx, draft_message_id)
        .await
        .unwrap();

    // Check draft is back in drafts folder.
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert!(!draft_message.label_ids.contains(&LabelId::all_scheduled()));
    assert!(!draft_message.label_ids.contains(&LabelId::outbox()));
    assert!(draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(draft_message.label_ids.contains(&LabelId::all_drafts()));
    assert!(!draft_message.is_scheduled_for_send());
    // Time of the message should be changed.
    assert_ne!(draft_message.time, delivery_time.timestamp().unsigned_abs());
}

#[tokio::test]
async fn cancel_schedule_send_after_api_request_succeeded() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let delivery_time = Local
        .from_local_datetime(&NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2025, 5, 2).unwrap(),
            NaiveTime::from_hms_opt(14, 10, 0).unwrap(),
        ))
        .unwrap();
    let mut api_message = message_body_test_message_simple();
    let mut undo_sent_response_message = api_message.clone();
    api_message.metadata.time = delivery_time.timestamp().unsigned_abs();
    undo_sent_response_message
        .metadata
        .label_ids
        .push(LabelId::drafts());
    undo_sent_response_message
        .metadata
        .label_ids
        .push(LabelId::all_drafts());

    let params = draft_test_params();
    ctx.setup_user(params.clone()).await;
    ctx.mock_undo_send(
        api_message.metadata.id.clone(),
        Ok(PostCancelSendResponse {
            message: undo_sent_response_message.metadata,
        }),
    )
    .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    api_message
        .metadata
        .label_ids
        .push(LabelId::all_scheduled());
    api_message.metadata.flags |= MessageFlags::SCHEDULED_SEND;
    let mut tether = user_ctx.user_stash().connection();
    let message = tether
        .tx::<_, _, MailContextError>(async |tx: &Bond<'_>| {
            let mut message = Message::from_api_metadata(api_message.metadata, tx).await?;
            message.save(tx).await?;
            Ok(message)
        })
        .await
        .unwrap();

    let previous_send_time = Draft::cancel_schedule_send(&user_ctx, message.local_id.unwrap())
        .await
        .unwrap();
    let message = Message::load(message.local_id.unwrap(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(message.label_ids.contains(&LabelId::drafts()));
    assert!(message.label_ids.contains(&LabelId::all_drafts()));
    assert!(!message.label_ids.contains(&LabelId::all_scheduled()));
    assert!(!message.is_scheduled_for_send());
    assert_eq!(delivery_time, previous_send_time);
}

#[tokio::test]
async fn cancel_schedule_send_on_already_sent_message() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let mut api_message = message_body_test_message_simple();

    let params = draft_test_params();
    ctx.setup_user(params.clone()).await;
    ctx.mock_undo_send(
        api_message.metadata.id.clone(),
        Err(ApiErrorInfo {
            code: Mail::MessageAlreadySent as u32,
            error: None,
            details: None,
        }),
    )
    .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    api_message
        .metadata
        .label_ids
        .push(LabelId::all_scheduled());
    api_message.metadata.flags |= MessageFlags::SCHEDULED_SEND;
    let mut tether = user_ctx.user_stash().connection();
    let message = tether
        .tx::<_, _, MailContextError>(async |tx: &Bond<'_>| {
            let mut message = Message::from_api_metadata(api_message.metadata, tx).await?;
            message.save(tx).await?;
            Ok(message)
        })
        .await
        .unwrap();

    let err = Draft::cancel_schedule_send(&user_ctx, message.local_id.unwrap())
        .await
        .unwrap_err();
    matches!(
        err,
        MailContextError::Draft(draft::Error::CancelScheduleSend(
            draft::CancelScheduleSendError::AlreadySent(_)
        ))
    );
}

async fn send_fails_if_recipient_is_not_valid_impl(
    api_error_code: u32,
) -> (Arc<anyhow::Error>, LocalMessageId, Arc<MailUserContext>) {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();

    let mut message = message_body_test_message_simple();
    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".to_string(),
        is_proton: false,
        name: "".to_string(),
        group: None,
    });
    let mut sent_message = message.clone();
    message.metadata.label_ids.push(LabelId::drafts());
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
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
    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let mut draft = Draft::empty(user_ctx.user_stash()).await.unwrap();
    draft
        .to_list
        .add_single(RecipientEntry {
            email: "foo@bar.com".into(),
            display_name: MaybeEmptyString(None),
        })
        .unwrap();

    draft
        .send(user_ctx.action_queue(), &user_ctx.user_stash().connection())
        .await
        .unwrap();

    // Execute action.
    let err = MailContextError::from(user_ctx.execute_all_send_actions().await.unwrap_err());
    let MailContextError::QueuedAction(QueuedError::Action(err, _)) = err else {
        panic!("invalid error");
    };

    (
        err,
        draft
            .message_id(&user_ctx.user_stash().connection())
            .await
            .unwrap()
            .unwrap(),
        user_ctx,
    )
}

fn draft_test_params() -> TestParams {
    draft_test_params_impl(None)
}
fn draft_test_params_impl(mime_type: Option<MimeType>) -> TestParams {
    let mut mail_settings = message_body_test_mail_settings();
    if let Some(mime_type) = mime_type {
        mail_settings.draft_mime_type = mime_type.into();
    }
    mail_settings.delay_send_seconds = SEND_DELAY_SECONDS;
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
        id: AddressId::from("GIBBERISH TEST ID"),
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

const SEND_DELAY_SECONDS: u32 = 60;

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

fn default_mock_send_params() -> TestDraftSendRequest {
    TestDraftSendRequest {
        expiration_time: None,
        expires_in: None,
        auto_save_contacts: Some(true),
        delay_seconds: Some(SEND_DELAY_SECONDS.into()),
        delivery_time: None,
    }
}

fn default_mock_schedule_send_params(delivery_time: u64) -> TestDraftSendRequest {
    TestDraftSendRequest {
        expiration_time: None,
        expires_in: None,
        auto_save_contacts: Some(true),
        delay_seconds: Some(SEND_DELAY_SECONDS.into()),
        delivery_time: Some(delivery_time),
    }
}
