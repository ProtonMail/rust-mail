use super::drafts_common::{self, draft_message};
use chrono::{Days, Local, Months, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use indoc::formatdoc;
use proton_action_queue::queue::{ActionError, AsActionError, QueuedError};
use proton_core_api::consts::{CoreBundle, Mail};
use proton_core_api::services::proton::ContactFull;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_api::services::proton::{
    Action, ContactCard as ApiContactCard, ContactEmail as ApiContactEmail, ContactEmailId,
    ContactId, ContactSendingPreferences as ApiContactSendingPreferences, ContactUID, EventId,
    GetKeysAllResponse,
};
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressFlags as ApiAddressFlags,
    AddressSignedKeyList as ApiAddressSignedKeyList, AddressStatus as ApiAddressStatus,
    AddressType as ApiAddressType,
};
use proton_core_api::services::proton::{AddressId, LabelId, UserId};
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::models::{Contact, ModelExtension};
use proton_crypto_account::contacts::{ContactCardType, EncryptableAndSignableCard};
use proton_crypto_account::keys::{ArmoredPrivateKey, EncryptedKeyToken, KeyTokenSignature};
use proton_crypto_account::proton_crypto::new_pgp_provider;
use proton_crypto_inbox::keys::PackageCryptoType;
use proton_crypto_inbox::message::EncryptedDraft;
use proton_crypto_inbox::proton_crypto_account::keys::{
    AddressKeys as ApiAddressKeys, KeyFlag, KeyId, LockedKey,
};
use proton_mail_api::services::proton::prelude::{MailEvent, MessageEvent, PostCancelSendResponse};
use proton_mail_api::services::proton::request_data::{
    DraftAttachmentKeyPackets, DraftParams, DraftRecipient, DraftSender,
};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationLabel, MessageFlags, MessageRecipient,
};
use proton_mail_common::datatypes::LocalMessageId;
use proton_mail_common::datatypes::{MimeType, SystemLabelId};
use proton_mail_common::draft::compose::DEFAULT_SUBJECT;
use proton_mail_common::draft::observers::{DraftSendResultWatcher, DraftSendResultWatcherMode};
use proton_mail_common::draft::recipients::RecipientEntry;
use proton_mail_common::draft::{Draft, DraftExpirationTime, RecipientGroupId};
use proton_mail_common::models::{
    DraftSendFailure, DraftSendFailureSend, DraftSendResult, DraftSendResultOrigin, MailSettings,
    Message, MessageBodyMetadata, MessageMimeType,
};
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::message_body::*;
use proton_mail_common::test_utils::messages::{
    TestDraftAuthInput, TestDraftSendAddressSubPackage, TestDraftSendPackage, TestDraftSendRequest,
};
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use proton_mail_common::{MailContextError, MailUserContext, draft};
use secrecy::ExposeSecret;
use stash::orm::Model;
use stash::stash::{Bond, StashError};
use std::sync::Arc;
use std::time::Duration;
use velcro::hash_map;

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
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
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
        display_snoozed_reminder: false,
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
        ..ApiConversation::test_default()
    };

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        None,
        Some(DraftAttachmentKeyPackets::new()),
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

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft.save().await.unwrap();

    // Save at least once so we can retrieve the message id.
    user_ctx.execute_all_send_actions().await.unwrap();

    // get draft message id.
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    draft.send().await.unwrap();

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

    let tether = user_ctx.user_stash().connection().await.unwrap();

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
    assert!(send_result.has_send_action);
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
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
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
        display_snoozed_reminder: false,
        expiration_time: 0,
        labels: vec![ConversationLabel {
            id: LabelId::sent(),
            context_num_messages: 1,
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        }],
        num_attachments: 0,
        num_messages: 1,
        subject: sent_message.metadata.subject.clone(),
        ..ApiConversation::test_default()
    };

    let expected_draft_params = expected_create_draft_params();
    let delivery_time = Local::now().checked_add_months(Months::new(1)).unwrap();

    ctx.setup_user(params.clone()).await;

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        None,
        Some(DraftAttachmentKeyPackets::new()),
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

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft.save().await.unwrap();

    // Save at least once so we can retrieve the message id.
    user_ctx.execute_all_send_actions().await.unwrap();

    // get draft message id.
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    draft.schedule_send(delivery_time).await.unwrap();

    // Check draft is in outbox.
    let mut draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert!(draft_message.label_ids.contains(&LabelId::all_scheduled()));
    assert!(!draft_message.label_ids.contains(&LabelId::outbox()));
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(!draft_message.label_ids.contains(&LabelId::all_drafts()));

    // Time of the message should match the delivery time.
    assert_eq!(draft_message.time, delivery_time.into());

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();
    draft_message.reload(&tether).await.unwrap();

    // Check message is in the sent folder
    assert_eq!(
        draft_message.label_ids,
        vec![LabelId::inbox(), LabelId::all_scheduled()] // No all_drafts
    );
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
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
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
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft.schedule_send(delivery_time).await.unwrap();

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
            draft::SendError::ScheduleSendExpired,
        )))
    ));

    // get draft message id.
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

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

    let send_result =
        DraftSendResult::find_by_id(local_id, &ctx.user_stash().connection().await.unwrap())
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

    let send_result =
        DraftSendResult::find_by_id(local_id, &ctx.user_stash().connection().await.unwrap())
            .await
            .unwrap()
            .unwrap();

    let draft_message = Message::find_by_id(
        send_result.local_message_id,
        &ctx.user_stash().connection().await.unwrap(),
    )
    .await
    .unwrap()
    .unwrap();

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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft.send().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap_err();

    let tether = user_ctx.user_stash().connection().await.unwrap();

    let send_result =
        DraftSendResult::find_by_id(draft.message_id().await.unwrap().unwrap(), &tether)
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
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
        group: None,
    });

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft.save().await.unwrap();

    // Save at least once so we can retrieve the message id.
    draft.send().await.unwrap();

    let result = draft.save().await;

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
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
        group: None,
    });

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        None,
        Some(DraftAttachmentKeyPackets::new()),
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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft.send().await.unwrap();

    let mut observer = DraftSendResultWatcher::new(
        user_ctx.user_stash().clone(),
        DraftSendResultWatcherMode::All,
    )
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

    let user_ctx = ctx.mail_user_context().await;
    let message = message_body_test_message_simple();
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let message = tether
        .tx::<_, _, MailContextError>(async |tx: &Bond<'_>| {
            let mut message = Message::from_api_metadata(message.metadata, tx).await?;
            message.save(tx).await?;
            Ok(message)
        })
        .await
        .unwrap();

    let err = Draft::cancel_schedule_send(&user_ctx, message.id())
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
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
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

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft.schedule_send(delivery_time).await.unwrap();

    let draft_message_id = draft.message_id().await.unwrap().unwrap();

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
    assert_ne!(draft_message.time, delivery_time.into());
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

    let user_ctx = ctx.mail_user_context().await;

    api_message
        .metadata
        .label_ids
        .push(LabelId::all_scheduled());

    api_message.metadata.flags |= MessageFlags::SCHEDULED_SEND;

    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let message = tether
        .tx::<_, _, MailContextError>(async |tx: &Bond<'_>| {
            let mut message = Message::from_api_metadata(api_message.metadata, tx).await?;
            message.save(tx).await?;
            Ok(message)
        })
        .await
        .unwrap();

    let previous_send_time = Draft::cancel_schedule_send(&user_ctx, message.id())
        .await
        .unwrap();

    let message = Message::load(message.id(), &tether).await.unwrap().unwrap();

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

    let user_ctx = ctx.mail_user_context().await;

    api_message
        .metadata
        .label_ids
        .push(LabelId::all_scheduled());

    api_message.metadata.flags |= MessageFlags::SCHEDULED_SEND;

    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let message = tether
        .tx::<_, _, MailContextError>(async |tx: &Bond<'_>| {
            let mut message = Message::from_api_metadata(api_message.metadata, tx).await?;
            message.save(tx).await?;
            Ok(message)
        })
        .await
        .unwrap();

    let err = Draft::cancel_schedule_send(&user_ctx, message.id())
        .await
        .unwrap_err();

    matches!(
        err,
        MailContextError::Draft(draft::Error::CancelScheduleSend(
            draft::CancelScheduleSendError::AlreadySent(_)
        ))
    );
}

#[tokio::test]
async fn schedule_send_message_limit() {
    // There can only be up to a 100 scheduled messages
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = draft_test_params();

    params.message_count.push(
        proton_mail_api::services::proton::response_data::MessageCount {
            label_id: LabelId::all_scheduled(),
            total: 100,
            unread: 0,
        },
    );

    let mut message = message_body_test_message_simple();

    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
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

    let delivery_time = Local::now().checked_sub_days(Days::new(2)).unwrap();

    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    let result = draft.schedule_send(delivery_time).await;

    assert!(matches!(
        result,
        Err(MailContextError::Draft(draft::Error::Send(
            draft::SendError::ScheduleSendMessageLimitExceeded,
        )))
    ));
}

#[tokio::test]
async fn message_sent_from_another_session_should_move_draft_to_sent_folder() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = drafts_common::draft_test_params();
    let message = draft_message();
    let expected_draft_params = drafts_common::expected_create_draft_params();

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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Simulate event loop update
    let mut sent_message = message.clone();

    sent_message.metadata.label_ids.clear();
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.label_ids.push(LabelId::all_mail());
    sent_message.metadata.flags = MessageFlags::SENT;

    user_ctx
        .apply_event(
            MailEvent {
                event_id: EventId::from("My Event ID"),
                labels: None,
                conversation_counts: None,
                conversations: None,
                incoming_defaults: None,
                mail_settings: None,
                message_counts: None,
                messages: Some(vec![MessageEvent {
                    id: message.metadata.id.clone(),
                    action: Action::Update,
                    message: Some(sent_message.metadata),
                }]),
                refresh: 0,
                has_more: false,
            }
            .into(),
        )
        .await
        .unwrap();

    // Load the draft.
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert_eq!(draft_message.remote_id, Some(message.metadata.id));

    // Check the draft has the draft label.
    assert!(!draft_message.label_ids.contains(&LabelId::drafts()));
    assert!(!draft_message.label_ids.contains(&LabelId::all_drafts()));
    assert!(draft_message.label_ids.contains(&LabelId::all_mail()));
    assert!(draft_message.label_ids.contains(&LabelId::sent()));
    assert!(!draft_message.is_draft());
}

#[tokio::test]
async fn message_sent_from_another_session_should_refetch_message() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let mut params = drafts_common::draft_test_params();
    let mut message1 = draft_message();

    // Remove the signature and make message plain so that message body is not contaminated
    message1.body.mime_type = MimeType::TextPlain.into();
    params.addresses[0].signature = String::new();

    let mut message2 = message1.clone();
    message2.body.body = formatdoc! {
"-----BEGIN PGP MESSAGE-----

wV4DGS71hsmM2EQSAQdAQbrUEnSKP+ePivt6gEVpZKyVL7nvyVNgMkxzpXEC01Ew
T+WzP5pNYZyfpiIiOhpAXLxxCZXh8ybiPNlPknYEUhPPZ/5m6cnEPT8uNXvi21kB
0sBjASJB8qaKq6/6ccjFBB8yH0FFgebo+9J2eaZmGtxDznz/LKajWa6HOr/LDjYv
VNCSfn80Zg1Zp+E0cVjbVvgiyy+oNLqy8DqvDFOIhm6QoxSBSW9U0nek0YA3QdkJ
ItmA8iKGTdQL5GXUC5QrKbD634mZQlogYWOLdrdhDjQZ6QjPzHjcEKxGDTCuEAsW
pK0Grd8zcJY/t8kZmy4Owm1cFia7u/8zAmMDgL0yMCKRwm+5Hpeg9RghFHIIabY7
M+PK763FJHYgYm3oeXPv+VayrM8lkwLiiSwaxHXtzh2HhR5k0nhjgoozQuMoupUz
1gPNzG+CWKxgFyzhvkIUeHb17IEe4VtGjInWrqLrAI7MY/Xg5cEvIvTWGIj9wCfU
1hzGEWHV
=IFmt
-----END PGP MESSAGE-----"};

    ctx.setup_user(params.clone()).await;
    ctx.mock_create_draft_no_validation(message1.clone()).await;
    ctx.mock_get_message_with_expected(&message2.metadata.id, message2.clone(), 1)
        .await;

    // Add some other label ids to this message to make sure they are skipped.
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();
    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();
    draft
        .set_mime_type(MessageMimeType::TextPlain)
        .await
        .unwrap();
    draft
        .set_body(String::from("Nobody expects"))
        .await
        .unwrap();
    draft.save().await.unwrap();

    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    let body = Message::message_body(&user_ctx, draft_message_id)
        .await
        .unwrap();

    assert_eq!(body.body, "Nobody expects");

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Simulate event loop update
    let mut sent_message = message1.clone();

    sent_message.metadata.label_ids.clear();
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.label_ids.push(LabelId::all_mail());
    sent_message.metadata.flags = MessageFlags::SENT;

    user_ctx
        .apply_event(
            MailEvent {
                event_id: EventId::from("My Event ID"),
                labels: None,
                conversation_counts: None,
                conversations: None,
                incoming_defaults: None,
                mail_settings: None,
                message_counts: None,
                messages: Some(vec![MessageEvent {
                    id: message1.metadata.id.clone(),
                    action: Action::Update,
                    message: Some(sent_message.metadata),
                }]),
                refresh: 0,
                has_more: false,
            }
            .into(),
        )
        .await
        .unwrap();

    // Load the draft.
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert_eq!(draft_message.remote_id, Some(message1.metadata.id));

    let body = Message::message_body(&user_ctx, draft_message_id)
        .await
        .unwrap();

    assert_eq!(body.body, "Nobody expects the spanish inquisition");
}

#[tokio::test]
async fn already_sent_from_even_update() {
    // gracefully handle a message already sent from another session via an event update.
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
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
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    ctx.mock_update_draft(
        message.metadata.id.clone(),
        expected_draft_params,
        message.clone(),
        DraftAttachmentKeyPackets::new(),
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    ctx.mock_send_draft_failure(
        message.metadata.id.clone(),
        ApiErrorInfo {
            code: Mail::MessageAlreadySent as u32,
            error: None,
            details: None,
        },
    )
    .await;

    draft.save().await.unwrap();

    // Save at least once so we can retrieve the message id.
    user_ctx.execute_all_send_actions().await.unwrap();

    draft.send().await.unwrap();

    // Simulate Event update
    user_ctx
        .apply_event(
            MailEvent {
                event_id: EventId::from("Event"),
                labels: None,
                conversation_counts: None,
                conversations: None,
                incoming_defaults: None,
                mail_settings: None,
                message_counts: None,
                messages: Some(vec![MessageEvent {
                    id: sent_message.metadata.id.clone(),
                    action: Action::Update,
                    message: Some(sent_message.metadata),
                }]),
                refresh: 0,
                has_more: false,
            }
            .into(),
        )
        .await
        .unwrap();

    //execute should not fail
    user_ctx.execute_all_send_actions().await.unwrap();
}

#[tokio::test]
async fn send_external_with_password() {
    let modulus = "-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\nK1PSamH/akNYFuWcErjkcbASp3Cot0Y6HfefGGbuHNKNlBTcv+SaLxZOSj8cV0A2N/NsNit7DUBiBGcKVNvk/0zSDWWFWKYcE9EPs4vSTbf/dqW5GYyIo1l8wBzIItivnTD5xQC4smJSYBIFJpVGuvtbDrDZI0xb0P+FVB5iFDTyPRE1J+ugZK+4QZczLJcv2/UG50gu9pi7R+rhYE/Q/4xCNpBZLp8mpFHpIVgj95auS2mILKkQS6xN7DyNLDuJjZF6++Qg1hxi38/d6NiFbMFgKlVHhKAFj5TPfKtVnqmlJmzeVgOCPc52cRfLRTDjEnDsoaa4MmsKC5gT9kNanQ==\n-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\nComment: https://protonmail.com\n\nwl4EARYIABAFAlwB1j4JEDUFhcTpUY8mAACghgEAotYZ/7iVaLKe52tP4CGF\nmdAAq2Dc6a7YLOnr4QLxC/8A/1UdoQQ/8PCueC41KEsrVktWSp1rB4lF4IvT\ntPvUc50G\n=+Zbf\n-----END PGP SIGNATURE-----\n";

    let modulus_id =
        "3ZJQXMBeonVrGHGEnuWG5zs0NHn8UNH8UH0TNswNWQYZJ10Fwp8vQVBGMHnmpWKmHKF6VlyMXCiMagSh8CGhkg==";

    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
        group: None,
    });

    let mut sent_message = message.clone();

    message.metadata.label_ids.push(LabelId::drafts());
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let mut send_params = default_mock_send_params();

    send_params.packages.push(TestDraftSendPackage {
        addresses: hash_map! {"foo@bar.com".to_string(): TestDraftSendAddressSubPackage{
                address_type: PackageCryptoType::EncryptedOutside,
                auth: Some(TestDraftAuthInput{modulus_id: modulus_id.to_string()}),
        }},
    });

    let sent_conversation = ApiConversation {
        id: message.metadata.conversation_id.clone(),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snoozed_reminder: false,
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
        ..ApiConversation::test_default()
    };

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        None,
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    ctx.mock_send_draft(
        message.metadata.id.clone(),
        send_params,
        sent_message.clone(),
        sent_conversation,
        (Utc::now().timestamp() + SEND_DELAY_SECONDS as i64).unsigned_abs(),
    )
    .await;

    ctx.core_test_context()
        .mock_get_auth(modulus_id.to_owned(), modulus.to_owned())
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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft
        .set_password("password", Some("hint".into()))
        .await
        .unwrap();

    let eo_data = draft.get_password().await.unwrap().unwrap();
    assert_eq!(eo_data.password.expose_secret(), "password");
    assert_eq!(eo_data.password_hint.as_deref(), Some("hint"));

    draft.send().await.unwrap();

    user_ctx.execute_all_send_actions().await.unwrap();
}

#[tokio::test]
async fn send_with_expiration() {
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
    let expiration_time = Local::now().checked_add_days(Days::new(10)).unwrap();
    let expiration_timestamp = UnixTimestamp::from(expiration_time);

    let mut message = message_body_test_message_simple();

    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
        group: None,
    });

    let mut sent_message = message.clone();

    message.metadata.label_ids.push(LabelId::drafts());
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);
    sent_message.body.header = "Fancy new header".to_owned();
    sent_message.metadata.expiration_time = expiration_timestamp.as_u64();

    let sent_conversation = ApiConversation {
        id: message.metadata.conversation_id.clone(),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snoozed_reminder: false,
        expiration_time: expiration_timestamp.as_u64(),
        labels: vec![ConversationLabel {
            id: LabelId::sent(),
            context_expiration_time: expiration_timestamp.as_u64(),
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
        ..ApiConversation::test_default()
    };

    let expected_draft_params = expected_create_draft_params();
    let mut send_params = default_mock_send_params();
    send_params.expiration_time = Some(expiration_timestamp.as_u64());

    ctx.setup_user(params.clone()).await;

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        None,
        Some(DraftAttachmentKeyPackets::new()),
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
        send_params,
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

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft
        .set_expiration_time(DraftExpirationTime::Custom(expiration_time))
        .await
        .unwrap();

    draft.save().await.unwrap();

    // Save at least once so we can retrieve the message id.
    user_ctx.execute_all_send_actions().await.unwrap();

    // get draft message id.
    let draft_message_id = draft.message_id().await.unwrap().unwrap();

    // Check expiration is set
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    // Expiration time should not be set on drafts.
    assert_eq!(draft_message.expiration_time, UnixTimestamp::new(0));

    draft.send().await.unwrap();

    user_ctx.execute_all_send_actions().await.unwrap();

    // Check draft is in outbox.
    let draft_message = Message::load(draft_message_id, &tether)
        .await
        .unwrap()
        .expect("failed to load message");

    assert_eq!(draft_message.expiration_time, expiration_timestamp);
}

#[tokio::test]
async fn send_external_with_password_even_if_contact_has_pgp_mime_encryption() {
    // We should always use password encryption even if the contact has
    // a pgp mim encryption key.
    let modulus = "-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\nK1PSamH/akNYFuWcErjkcbASp3Cot0Y6HfefGGbuHNKNlBTcv+SaLxZOSj8cV0A2N/NsNit7DUBiBGcKVNvk/0zSDWWFWKYcE9EPs4vSTbf/dqW5GYyIo1l8wBzIItivnTD5xQC4smJSYBIFJpVGuvtbDrDZI0xb0P+FVB5iFDTyPRE1J+ugZK+4QZczLJcv2/UG50gu9pi7R+rhYE/Q/4xCNpBZLp8mpFHpIVgj95auS2mILKkQS6xN7DyNLDuJjZF6++Qg1hxi38/d6NiFbMFgKlVHhKAFj5TPfKtVnqmlJmzeVgOCPc52cRfLRTDjEnDsoaa4MmsKC5gT9kNanQ==\n-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\nComment: https://protonmail.com\n\nwl4EARYIABAFAlwB1j4JEDUFhcTpUY8mAACghgEAotYZ/7iVaLKe52tP4CGF\nmdAAq2Dc6a7YLOnr4QLxC/8A/1UdoQQ/8PCueC41KEsrVktWSp1rB4lF4IvT\ntPvUc50G\n=+Zbf\n-----END PGP SIGNATURE-----\n";

    let modulus_id =
        "3ZJQXMBeonVrGHGEnuWG5zs0NHn8UNH8UH0TNswNWQYZJ10Fwp8vQVBGMHnmpWKmHKF6VlyMXCiMagSh8CGhkg==";

    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.to_list.push(MessageRecipient {
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
        group: None,
    });

    let mut sent_message = message.clone();

    message.metadata.label_ids.push(LabelId::drafts());
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);
    sent_message.body.header = "Fancy new header".to_owned();

    let mut send_params = default_mock_send_params();

    send_params.packages.push(TestDraftSendPackage {
        addresses: hash_map! {"foo@bar.com".to_string(): TestDraftSendAddressSubPackage{
                address_type: PackageCryptoType::EncryptedOutside,
                auth: Some(TestDraftAuthInput{modulus_id: modulus_id.to_string()}),
        }},
    });

    let sent_conversation = ApiConversation {
        id: message.metadata.conversation_id.clone(),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snoozed_reminder: false,
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
        ..ApiConversation::test_default()
    };

    let expected_draft_params = expected_create_draft_params();

    ctx.setup_user(params.clone()).await;

    ctx.mock_create_draft(
        expected_draft_params.clone(),
        None,
        message.clone(),
        None,
        Some(DraftAttachmentKeyPackets::new()),
    )
    .await;

    ctx.mock_send_draft(
        message.metadata.id.clone(),
        send_params,
        sent_message.clone(),
        sent_conversation,
        (Utc::now().timestamp() + SEND_DELAY_SECONDS as i64).unsigned_abs(),
    )
    .await;

    ctx.core_test_context()
        .mock_get_auth(modulus_id.to_owned(), modulus.to_owned())
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

    let user_ctx = ctx.mail_user_context().await;

    let vcard_data = "BEGIN:VCARD\r\nVERSION:4.0\r\nITEM1.EMAIL;PREF=1:foo@bar.com\r\nFN;PREF=1:Mime \r\nUID:proton-web-9c3fda06-3426-fdef-ea4a-aa1b336b085d\r\nPRODID;VALUE=TEXT:-//ProtonMail//ProtonMail vCard 1.0.0//EN\r\nITEM1.KEY;PREF=1:data:application/pgp-keys;base64,xsFNBFzRQp0BEACqHCI6gicK4\r\n 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\r\nITEM1.X-PM-ENCRYPT:true\r\nITEM1.X-PM-SIGN:true\r\nITEM1.X-PM-SCHEME:pgp-mime\r\nEND:VCARD";
    let encrypted_vcard = EncryptableVcardStr(vcard_data);

    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let provider = new_pgp_provider();
    let user_keys = user_ctx
        .unlocked_user_keys(&provider, &tether)
        .await
        .unwrap();
    let signature = encrypted_vcard
        .sign_sync(&provider, user_keys.primary().unwrap())
        .unwrap();

    let api_contact = ContactFull {
        id: ContactId::from("CONTACT"),
        cards: vec![ApiContactCard {
            card_type: ContactCardType::Signed,
            data: vcard_data.into(),
            signature: Some(signature.0),
        }],
        contact_emails: vec![ApiContactEmail {
            id: ContactEmailId::from("EMAIL"),
            contact_id: ContactId::from("CONTACT"),
            canonical_email: "foo@bar.com".into(),
            contact_type: Default::default(),
            defaults: ApiContactSendingPreferences::Custom,
            email: "foo@bar.com".into(),
            is_proton: false,
            label_ids: Default::default(),
            last_used_time: Default::default(),
            name: "".to_string(),
            order: 0,
        }],
        create_time: 0,
        label_ids: Default::default(),
        modify_time: 0,
        name: "".to_string(),
        size: 0,
        uid: ContactUID::from("UID"),
    };

    ctx.core_test_context
        .mock_get_full_contact(api_contact.clone())
        .await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut contact = Contact::from(api_contact.clone());
            contact.save(tx).await?;
            for email in &mut contact.contact_emails {
                email.save(tx).await?;
            }
            Ok(contact)
        })
        .await
        .unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft
        .set_password("password", Some("hint".into()))
        .await
        .unwrap();

    let eo_data = draft.get_password().await.unwrap().unwrap();
    assert_eq!(eo_data.password.expose_secret(), "password");
    assert_eq!(eo_data.password_hint.as_deref(), Some("hint"));

    draft.send().await.unwrap();

    user_ctx.execute_all_send_actions().await.unwrap();
}

struct EncryptableVcardStr<'a>(&'a str);

impl EncryptableAndSignableCard for EncryptableVcardStr<'_> {
    fn plaintext_card_data(&self) -> &[u8] {
        self.0.as_bytes()
    }
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
        address: "foo@bar.com".into(),
        is_proton: false,
        name: "".into(),
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
        Some(DraftAttachmentKeyPackets::new()),
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

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft
        .add_single_recipient(
            RecipientGroupId::To,
            RecipientEntry {
                email: "foo@bar.com".into(),
                name: None,
            },
        )
        .await
        .unwrap();

    draft.send().await.unwrap();

    // Execute action.
    let err = MailContextError::from(user_ctx.execute_all_send_actions().await.unwrap_err());

    let MailContextError::QueuedAction(QueuedError::Action(err, _)) = err else {
        panic!("invalid error");
    };

    (err, draft.message_id().await.unwrap().unwrap(), user_ctx)
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
        flags: ApiAddressFlags::default(),
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
            address: address[0].email.clone().into(),
            name: address[0].display_name.clone().into(),
        },
        to_list: vec![DraftRecipient {
            address: "foo@bar.com".into(),
            name: String::new().into(),
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
        packages: vec![],
    }
}

fn default_mock_schedule_send_params(delivery_time: u64) -> TestDraftSendRequest {
    TestDraftSendRequest {
        expiration_time: None,
        expires_in: None,
        auto_save_contacts: Some(true),
        delay_seconds: Some(SEND_DELAY_SECONDS.into()),
        delivery_time: Some(delivery_time),
        packages: vec![],
    }
}
