use super::drafts_common::*;
use proton_core_api::consts::{General, Mail};
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_api::services::proton::{LabelId, UserId};
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::prelude::{
    DraftAttachmentKeyPackets, MessageFlags, OperationResult, PutMessagesDeleteResponse,
};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::draft::{Draft, ReplyMode};
use proton_mail_common::models::{Conversation, DraftMetadata, Message};
use proton_mail_common::test_utils::message_body::{
    TEST_USER_ID, message_body_test_message_simple, message_body_test_user_secret,
};
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;

#[tokio::test]
async fn discard_before_save_only_deletes_metadata() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.push(LabelId::drafts());
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.discard().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    assert!(
        DraftMetadata::find_by_id(
            draft.metadata_id,
            &user_ctx.user_stash().connection().await.unwrap()
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn discard_by_message_id() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.push(LabelId::drafts());
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.discard().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    assert!(
        DraftMetadata::find_by_id(
            draft.metadata_id,
            &user_ctx.user_stash().connection().await.unwrap()
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn discard_draft_after_save_marks_message_deleted() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.clear();
    message.metadata.label_ids.push(LabelId::drafts());

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

    ctx.mock_message_delete(
        [message.metadata.id.clone()],
        Some(LabelId::drafts()),
        PutMessagesDeleteResponse {
            responses: vec![OperationResult {
                id: message.metadata.id.clone(),
                response: ApiErrorInfo {
                    code: General::NoError as u32,
                    error: None,
                    details: None,
                },
            }],
        },
    )
    .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // queue discard.
    draft.discard().await.unwrap();

    // Check the message is marked as deleted.
    let message = Message::find_by_remote_id(
        message.metadata.id.clone(),
        &user_ctx.user_stash().connection().await.unwrap(),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(message.deleted);

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    Draft::open(&user_ctx, message.id())
        .await
        .expect_err("Should not work");
}

#[tokio::test]
async fn discard_draft_by_message_id() {
    // Same as `discard_draft_after_save_marks_message_deleted` but using standalone discard.
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.clear();
    message.metadata.label_ids.push(LabelId::drafts());

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

    ctx.mock_message_delete(
        [message.metadata.id.clone()],
        Some(LabelId::drafts()),
        PutMessagesDeleteResponse {
            responses: vec![OperationResult {
                id: message.metadata.id.clone(),
                response: ApiErrorInfo {
                    code: General::NoError as u32,
                    error: None,
                    details: None,
                },
            }],
        },
    )
    .await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    let message_id = draft.message_id().await.unwrap().unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // queue discard.
    Draft::action_discard(
        message_id,
        &user_ctx.user_stash().connection().await.unwrap(),
        user_ctx.action_queue(),
        user_ctx.origin(),
    )
    .await
    .unwrap();

    // Check the message is marked as deleted.
    let message = Message::find_by_remote_id(
        message.metadata.id.clone(),
        &user_ctx.user_stash().connection().await.unwrap(),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(message.deleted);

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    Draft::open(&user_ctx, message.id())
        .await
        .expect_err("Should not work");
}

#[tokio::test]
async fn discard_new_draft_after_cancelled_or_failed_save_action_deletes_local_data() {
    // We use cancel here to explicitly trigger the revert changes, but
    // this can also be achieved by having the execute remote part of `draft::save` fail. The
    // latter would require more setup.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();

    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.clear();
    message.metadata.label_ids.push(LabelId::drafts());

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    let action_id = draft.save().await.unwrap().id;

    let local_message_id = draft.message_id().await.unwrap().unwrap();
    let local_conversation_id = draft.conversation_id().await.unwrap().unwrap();

    // Cancel create draft, will leave the message and conversation there.
    user_ctx.action_queue().cancel(action_id).await.unwrap();

    // queue discard.
    draft.discard().await.unwrap();

    // Check the message is marked as deleted.
    let draft_message = Message::find_by_id(local_message_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(draft_message.deleted);

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Message is deleted.
    let draft_message = Message::find_by_id(local_message_id, &tether)
        .await
        .unwrap();

    assert!(draft_message.is_none());

    // Conversation is deleted.
    let conv_message = Conversation::find_by_id(local_conversation_id, &tether)
        .await
        .unwrap();

    assert!(conv_message.is_none());

    Draft::open(&user_ctx, local_message_id)
        .await
        .expect_err("Should not work");
}

#[tokio::test]
async fn delete_new_draft_after_cancelled_or_failed_save_action_deletes_local_data() {
    // We use cancel here to explicitly trigger the revert changes, but
    // this can also be achieved by having the execute remote part of `draft::save` fail. The
    // latter would require more setup.

    // This test is similar to discard, but we are simulating a delete message from
    // the list of messages in the draft label, which will not use the discard action.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.clear();
    message.metadata.label_ids.push(LabelId::drafts());

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    let action_id = draft.save().await.unwrap().id;

    let local_message_id = draft.message_id().await.unwrap().unwrap();
    let local_conversation_id = draft.conversation_id().await.unwrap().unwrap();

    // Cancel create draft, will leave the message and conversation there.
    user_ctx.action_queue().cancel(action_id).await.unwrap();

    // Use message delete rather than discard - simulates deleting from the draft message view.
    let local_draft_label_id = Label::remote_id_counterpart(LabelId::drafts(), &tether)
        .await
        .unwrap()
        .unwrap();

    Message::action_delete(
        user_ctx.action_queue(),
        local_draft_label_id,
        vec![local_message_id],
    )
    .await
    .unwrap();

    // Check the message is marked as deleted.
    let draft_message = Message::find_by_id(local_message_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(draft_message.deleted);

    // Execute action.
    user_ctx.execute_all_actions().await.unwrap();

    // Message is deleted.
    let draft_message = Message::find_by_id(local_message_id, &tether)
        .await
        .unwrap();

    assert!(draft_message.is_none());

    // Conversation is deleted.
    let conv_message = Conversation::find_by_id(local_conversation_id, &tether)
        .await
        .unwrap();

    assert!(conv_message.is_none());

    Draft::open(&user_ctx, local_message_id)
        .await
        .expect_err("Should not work");
}

#[tokio::test]
async fn discard_reply_draft_after_cancelled_or_failed_save_action_only_deletes_message() {
    // We use cancel here to explicitly trigger the revert changes, but
    // this can also be achieved by having the execute remote part of `draft::save` fail. The
    // latter would require more setup.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();

    let mut remote_existing_message = draft_message();

    remote_existing_message.metadata.sender.address = "me@proton.me".to_owned().into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    ctx.setup_user(params.clone()).await;

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    // Get the message body - required to reply to draft.
    Message::message_body(&user_ctx, existing_message.id())
        .await
        .unwrap();

    // Create draft.
    let draft = Draft::reply(&user_ctx, existing_message.id(), ReplyMode::All, true)
        .await
        .unwrap();

    let action_id = draft.save().await.unwrap().id;

    let local_message_id = draft.message_id().await.unwrap().unwrap();
    let local_conversation_id = draft.conversation_id().await.unwrap().unwrap();

    // Cancel create draft, will leave the message and conversation there.
    user_ctx.action_queue().cancel(action_id).await.unwrap();

    // queue discard.
    draft.discard().await.unwrap();

    // Check the message is marked as deleted.
    let draft_message = Message::find_by_id(local_message_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(draft_message.deleted);

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // Message is deleted.
    let draft_message = Message::find_by_id(local_message_id, &tether)
        .await
        .unwrap();

    assert!(draft_message.is_none());

    // Conversation is not deleted.
    let conv_message = Conversation::find_by_id(local_conversation_id, &tether)
        .await
        .unwrap();

    assert!(conv_message.is_some());

    Draft::open(&user_ctx, local_message_id)
        .await
        .expect_err("Should not work");
}

#[tokio::test]
async fn delete_reply_draft_after_cancelled_or_failed_save_action_only_deletes_message() {
    // We use cancel here to explicitly trigger the revert changes, but
    // this can also be achieved by having the execute remote part of `draft::save` fail. The
    // latter would require more setup.

    // This test is similar to discard, but we are simulating a delete message from
    // the list of messages in the draft label, which will not use the discard action.

    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.clear();
    message.metadata.label_ids.push(LabelId::drafts());

    let mut remote_existing_message = draft_message_with_attachments();

    remote_existing_message.metadata.sender.address = "me@proton.me".to_owned().into();
    remote_existing_message.metadata.id = "FancyRemoteId".into();
    remote_existing_message.metadata.flags |= MessageFlags::RECEIVED;

    ctx.setup_user(params.clone()).await;

    ctx.mock_get_message(
        &remote_existing_message.metadata.id,
        remote_existing_message.clone(),
    )
    .await;

    for attachment in &remote_existing_message.body.attachments {
        ctx.mock_maybe_get_attachment_data(attachment.id.clone(), vec![])
            .await;
    }

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let (mut existing_message, _, _) =
        Message::from_api_data(remote_existing_message.clone(), &tether)
            .await
            .unwrap();

    tether
        .tx(async |tx| existing_message.save(tx).await)
        .await
        .unwrap();

    // Get the message body - required to reply to draft.
    Message::message_body(&user_ctx, existing_message.id())
        .await
        .unwrap();

    // Create draft.
    let draft = Draft::reply(&user_ctx, existing_message.id(), ReplyMode::All, true)
        .await
        .unwrap();

    let action_id = draft.save().await.unwrap().id;

    let local_message_id = draft.message_id().await.unwrap().unwrap();
    let local_conversation_id = draft.conversation_id().await.unwrap().unwrap();

    // Cancel create draft, will leave the message and conversation there.
    user_ctx.action_queue().cancel(action_id).await.unwrap();

    // Use message delete rather than discard - simulates deleting from the draft message view.
    let local_draft_label_id = Label::remote_id_counterpart(LabelId::drafts(), &tether)
        .await
        .unwrap()
        .unwrap();

    Message::action_delete(
        user_ctx.action_queue(),
        local_draft_label_id,
        vec![local_message_id],
    )
    .await
    .unwrap();

    // Check the message is marked as deleted.
    let draft_message = Message::find_by_id(local_message_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(draft_message.deleted);

    // Execute action.
    user_ctx.execute_all_actions().await.unwrap();

    // Message is deleted.
    let draft_message = Message::find_by_id(local_message_id, &tether)
        .await
        .unwrap();

    assert!(draft_message.is_none());

    // Conversation is not deleted.
    let conv_message = Conversation::find_by_id(local_conversation_id, &tether)
        .await
        .unwrap();

    assert!(conv_message.is_some());

    Draft::open(&user_ctx, local_message_id)
        .await
        .expect_err("Should not work");
}

#[tokio::test]
async fn discard_draft_failure_undeletes_message() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = draft_test_params();
    let mut message = message_body_test_message_simple();

    message.metadata.label_ids.clear();
    message.metadata.label_ids.push(LabelId::drafts());

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

    ctx.mock_message_delete(
        [message.metadata.id.clone()],
        Some(LabelId::drafts()),
        PutMessagesDeleteResponse {
            responses: vec![OperationResult {
                id: message.metadata.id.clone(),
                response: ApiErrorInfo {
                    code: Mail::MessageAlreadySent as u32,
                    error: None,
                    details: None,
                },
            }],
        },
    )
    .await;

    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;

    // Create draft.
    let draft = Draft::empty(&user_ctx).await.unwrap();

    draft.save().await.unwrap();

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap();

    // queue discard.
    draft.discard().await.unwrap();

    // Check the message is marked as deleted.
    let local_message = Message::find_by_remote_id(
        message.metadata.id.clone(),
        &user_ctx.user_stash().connection().await.unwrap(),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(local_message.deleted);

    // Execute action.
    user_ctx.execute_all_send_actions().await.unwrap_err();

    let message = Message::find_by_remote_id(
        message.metadata.id.clone(),
        &user_ctx.user_stash().connection().await.unwrap(),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(!message.deleted);
}
