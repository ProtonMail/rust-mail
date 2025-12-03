use proton_action_queue::queue::{ActionError, AsActionError, QueuedError};
use proton_core_api::consts::Mail;
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_api::services::proton::{LabelId, UserId};
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::prelude::{MessageFlags, PostCancelSendResponse};
use proton_mail_common::MailContextError;
use proton_mail_common::actions::draft::UndoSend;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::draft::{Draft, Error, UndoError};
use proton_mail_common::models::Message;
use proton_mail_common::test_utils::message_body::{
    TEST_USER_ID, message_body_test_message_simple, message_body_test_params,
    message_body_test_user_secret,
};
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;

#[tokio::test]
async fn draft_undo_send() {
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = message_body_test_params();

    let mut sent_message = message_body_test_message_simple();
    sent_message.metadata.label_ids.clear();
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);

    let mut undo_message = sent_message.metadata.clone();
    undo_message.label_ids.clear();
    undo_message.label_ids.push(LabelId::all_drafts());
    undo_message.label_ids.push(LabelId::drafts());
    undo_message.flags.set(MessageFlags::SENT, false);

    ctx.setup_user(params.clone()).await;
    ctx.mock_undo_send(
        sent_message.metadata.id.clone(),
        Ok(PostCancelSendResponse {
            message: undo_message,
        }),
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut local_sent_message = Message::from_api_metadata(sent_message.metadata.clone(), &tether)
        .await
        .unwrap();

    tether
        .tx(async |tx| local_sent_message.save(tx).await)
        .await
        .unwrap();

    // Queue undo send action - The exposed API uses apply for faster reaction, but we
    // want to check intermediate state here.
    user_ctx
        .action_queue()
        .queue_action(UndoSend::new(local_sent_message.id()))
        .await
        .unwrap();

    let updated_local_message = Message::find_by_id(local_sent_message.id(), &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(
        updated_local_message
            .label_ids
            .contains(&LabelId::all_drafts())
    );
    assert!(updated_local_message.label_ids.contains(&LabelId::drafts()));
    assert!(!updated_local_message.label_ids.contains(&LabelId::sent()));
    assert!(
        !updated_local_message
            .flags
            .contains(MessageFlags::SENT.into())
    );

    // flush queue.
    user_ctx.execute_single_send_action().await.unwrap();
}

#[tokio::test]
async fn draft_undo_send_failure() {
    // Check that the message is put back into the sent folder
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;

    let params = message_body_test_params();

    let mut sent_message = message_body_test_message_simple();
    sent_message.metadata.label_ids.clear();
    sent_message.metadata.label_ids.push(LabelId::sent());
    sent_message.metadata.flags.set(MessageFlags::SENT, true);

    ctx.setup_user(params.clone()).await;
    ctx.mock_undo_send(
        sent_message.metadata.id.clone(),
        Err(ApiErrorInfo {
            code: Mail::MessageSentCanNoLongerBeUndone as u32,
            error: None,
            details: None,
        }),
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut local_sent_message = Message::from_api_metadata(sent_message.metadata.clone(), &tether)
        .await
        .unwrap();

    tether
        .tx(async |tx| local_sent_message.save(tx).await)
        .await
        .unwrap();

    // Que undo send action
    Draft::action_undo_send(user_ctx.action_queue(), local_sent_message.id())
        .await
        .unwrap();

    let err = user_ctx.execute_single_send_action().await.unwrap_err();

    let updated_local_message = Message::find_by_id(local_sent_message.id(), &tether)
        .await
        .unwrap()
        .unwrap();

    assert!(!updated_local_message.label_ids.contains(&LabelId::drafts()));
    assert!(
        !updated_local_message
            .label_ids
            .contains(&LabelId::all_drafts())
    );
    assert!(updated_local_message.label_ids.contains(&LabelId::sent()));
    assert!(
        updated_local_message
            .flags
            .contains(MessageFlags::SENT.into())
    );

    match err {
        QueuedError::Action(err, _) => {
            let action_err = err.as_action_error::<UndoSend>().unwrap();
            assert!(matches!(
                action_err,
                ActionError::Action(MailContextError::Draft(Error::Undo(
                    UndoError::SendCanNoLongerBeUndone
                )))
            ));
        }
        _ => panic!("Unexpected error"),
    }
}
