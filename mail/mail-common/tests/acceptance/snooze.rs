use chrono::{DateTime, Duration, Local};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LocalLabelId, SystemLabel, UnixTimestamp};
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_mail_common::actions::conversations::Snooze;
use proton_mail_common::datatypes::{MessageFlags, SystemLabelId};
use proton_mail_common::models::{Conversation, ConversationCounters, ConversationLabel, Message};
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::scroller::StoreLabeledModelMap as _;
use proton_mail_common::test_utils::test_context::{
    MailTestContext, MailUserContextTestExtension as _,
};
use proton_mail_common::test_utils::utils::create_address;
use proton_mail_common::{conv_id, conversation, message, msg_id};
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use velcro::hash_map;

struct TestData {
    conversation: Conversation,
    label_message: Message,
    sent_message: Message,
}

async fn setup_test_label(label_id: LocalLabelId, tether: &mut Tether) -> TestData {
    let remote_label_id = Label::local_id_counterpart(label_id, tether)
        .await
        .unwrap()
        .unwrap();
    // Create a conversation in inbox
    let mut conv_data = hash_map! {
        vec![remote_label_id.clone()]: vec![conversation!(remote_id: conv_id!("test_conv"))]
    };
    conv_data.save_to_database(tether).await;

    let conv = &conv_data.get(&vec![remote_label_id]).unwrap()[0];

    // Set up message
    let address = create_address(tether).await;
    let mut label_message = message!(
        remote_id: msg_id!("inbox_msg"),
        local_conversation_id: conv.local_id,
        remote_conversation_id: conv.remote_id.clone(),
        local_address_id: address.local_id.unwrap(),
        remote_address_id: address.remote_id.clone().unwrap(),
        flags: MessageFlags::RECEIVED
    );
    let mut sent_message = message!(
        remote_id: msg_id!("sent_msg"),
        local_conversation_id: conv.local_id,
        remote_conversation_id: conv.remote_id.clone(),
        local_address_id: address.local_id.unwrap(),
        remote_address_id: address.remote_id.unwrap(),
        flags: MessageFlags::SENT
    );

    // Set up counters for inbox
    let mut inbox_conv_counter = ConversationCounters::new(label_id);
    inbox_conv_counter.total = 1;

    tether
        .tx::<_, _, StashError>(async |tx| {
            inbox_conv_counter.save(tx).await?;

            // Important: we need to apply the labels to the messages to be able to snooze conversation
            label_message.save(tx).await?;
            Message::apply_label(label_id, vec![label_message.local_id.unwrap()], tx).await?;
            label_message.reload(tx).await?;
            sent_message.save(tx).await?;
            let sent = SystemLabel::Sent.load(tx).await.unwrap().unwrap();
            Message::apply_label(sent.id(), vec![sent_message.local_id.unwrap()], tx).await?;
            sent_message.reload(tx).await?;

            Ok(())
        })
        .await
        .unwrap();

    let conversation = Conversation::load(conv.id(), tether)
        .await
        .unwrap()
        .unwrap();

    TestData {
        conversation,
        label_message,
        sent_message,
    }
}

#[tokio::test]
async fn action_snooze_conversation_from_inbox_to_snoozed() {
    // Setup:
    // * create a conversation in inbox
    // * snooze the conversation until tomorrow
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    ctx.setup_user(params.clone()).await;

    // Create snooze time (1 hour from now)
    let snooze_time: DateTime<Local> = Local::now() + Duration::hours(1);
    let snooze_timestamp = UnixTimestamp::from(snooze_time);

    // Mock the API call for snoozing conversations
    ctx.mock_put_conversations_snooze(
        vec![conv_id!("test_conv").unwrap()],
        snooze_timestamp.as_u64(),
        vec![],
    )
    .await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let inbox = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();
    let TestData {
        conversation: conv,
        label_message: inbox_message,
        mut sent_message,
    } = setup_test_label(inbox.id(), &mut tether).await;
    let sent_remote_id = SystemLabel::Sent.remote_id();
    let expected_sent_msg_labels = sent_message.label_ids.clone();
    assert_eq!(expected_sent_msg_labels.len(), 1);
    assert_eq!(expected_sent_msg_labels[0], sent_remote_id);

    // Verify initial state - conversation is in inbox
    let conversations_in_inbox = Conversation::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_inbox.len(), 1);
    assert_eq!(conversations_in_inbox[0].id(), conv.local_id.unwrap());
    let messages_in_inbox = Message::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(messages_in_inbox.len(), 1);
    assert_eq!(messages_in_inbox[0].local_id, inbox_message.local_id,);

    // Action: Snooze the conversation
    let action = Snooze::new(inbox.id(), vec![conv.local_id.unwrap()], snooze_timestamp);

    user_ctx.action_queue().queue_action(action).await.unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation:
    // * conversation is no longer in inbox
    let conversations_in_inbox_after = Conversation::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_inbox_after.len(), 0);
    let messages_in_inbox_after = Message::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(messages_in_inbox_after.len(), 0);

    // * conversation is now in snoozed
    let snoozed = SystemLabel::Snoozed.load(&tether).await.unwrap().unwrap();
    let conversations_in_snoozed = Conversation::in_label(snoozed.id(), &tether).await.unwrap();

    assert_eq!(conversations_in_snoozed.len(), 1);

    let snoozed_conversation = &conversations_in_snoozed[0];
    assert_eq!(snoozed_conversation.id(), conv.local_id.unwrap());

    let messages_in_snoozed = Message::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(messages_in_snoozed.len(), 1);
    assert_eq!(messages_in_snoozed[0].local_id, inbox_message.local_id);

    // * conversation has the correct snooze time
    let snoozed_label = snoozed_conversation
        .labels
        .iter()
        .find(|label| label.remote_label_id == snoozed.remote_id)
        .expect("Conversation should have snoozed label");

    assert_eq!(snoozed_label.context_snooze_time, snooze_timestamp);

    // * message has the correct snooze time
    let snoozed_message = &messages_in_snoozed[0];
    assert_eq!(snoozed_message.snooze_time, snooze_timestamp);

    // * sent message is still in sent
    sent_message.reload(&tether).await.unwrap();
    assert_eq!(sent_message.label_ids, expected_sent_msg_labels);
}

#[tokio::test]
async fn unsnooze_conversation_from_snoozed_to_inbox() {
    // Setup:
    // * create a conversation in snoozed with a snooze time
    // * unsnooze the conversation back to inbox
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Get labels
    let snoozed = SystemLabel::Snoozed.load(&tether).await.unwrap().unwrap();
    let inbox = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();

    // Create a conversation in snoozed with snooze time
    let snooze_time: DateTime<Local> = Local::now() + Duration::hours(1);
    let snooze_timestamp: UnixTimestamp = UnixTimestamp::from(snooze_time);

    let TestData {
        conversation: conv,
        label_message: mut snoozed_message,
        sent_message: _,
    } = setup_test_label(snoozed.id(), &mut tether).await;

    // Set up the conversation with snooze time manually
    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut label =
                ConversationLabel::find_by_conversation_and_label_id(conv.id(), snoozed.id(), tx)
                    .await?
                    .unwrap();
            label.context_snooze_time = snooze_timestamp;
            label.save(tx).await?;
            snoozed_message.reload(tx).await?;

            Ok(())
        })
        .await
        .unwrap();

    // Verify initial state - conversation is in snoozed
    let conversations_in_snoozed = Conversation::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_snoozed.len(), 1);

    let messages_in_snoozed = Message::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(messages_in_snoozed.len(), 1);
    assert_eq!(messages_in_snoozed[0].local_id, snoozed_message.local_id);

    // Action: Unsnooze the conversation
    tether
        .tx(async |tx| Conversation::unsnooze(snoozed.id(), &[conv.id()], tx).await)
        .await
        .unwrap();

    // Validation:
    // * conversation is no longer in snoozed
    let conversations_in_snoozed_after =
        Conversation::in_label(snoozed.id(), &tether).await.unwrap();

    assert_eq!(conversations_in_snoozed_after.len(), 0);
    let messages_in_snoozed_after = Message::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(messages_in_snoozed_after.len(), 0);

    // * conversation is back in inbox
    let conversations_in_inbox = Conversation::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_inbox.len(), 1);

    let messages_in_inbox = Message::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(messages_in_inbox.len(), 1);
    assert_eq!(messages_in_inbox[0].local_id, snoozed_message.local_id);

    let unsnoozed_conversation = &conversations_in_inbox[0];
    assert_eq!(unsnoozed_conversation.id(), conv.local_id.unwrap());

    let unsnoozed_message = &messages_in_inbox[0];
    assert_eq!(unsnoozed_message.local_id, snoozed_message.local_id);

    // * snooze time is reset to context_time
    let inbox_label = unsnoozed_conversation
        .labels
        .iter()
        .find(|label| label.remote_label_id == Some(LabelId::inbox()))
        .expect("Conversation should have inbox label");

    assert_eq!(inbox_label.context_snooze_time, inbox_label.context_time);

    // * message has the correct snooze time
    let unsnoozed_message = &messages_in_inbox[0];
    assert_eq!(unsnoozed_message.snooze_time, UnixTimestamp::new(0));
}

#[tokio::test]
async fn action_unsnooze_conversation_from_snoozed_to_inbox() {
    // Setup:
    // * create a conversation in snoozed with a snooze time
    // * use the Unsnooze action to move it back to inbox
    // * verify both local and remote action behavior
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    ctx.setup_user(params.clone()).await;

    // Create snooze time (1 hour from now)
    let snooze_time: DateTime<Local> = Local::now() + Duration::hours(1);
    let snooze_timestamp: UnixTimestamp = UnixTimestamp::from(snooze_time);

    // Mock the API call for unsnoozing conversations
    ctx.mock_put_conversations_unsnooze(vec![conv_id!("test_conv").unwrap()], vec![])
        .await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Get labels
    let snoozed = SystemLabel::Snoozed.load(&tether).await.unwrap().unwrap();
    let inbox = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();

    // Set up the conversation with snooze time and counters
    let TestData {
        conversation: conv,
        label_message: snoozed_message,
        sent_message: _,
    } = setup_test_label(inbox.id(), &mut tether).await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Apply snooze to set the snooze time
            Conversation::snooze(inbox.id(), &[conv.id()], snooze_timestamp, tx)
                .await
                .unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Verify initial state - conversation is in snoozed with correct snooze time
    let conversations_in_snoozed = Conversation::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_snoozed.len(), 1);
    assert_eq!(conversations_in_snoozed[0].id(), conv.local_id.unwrap());

    let messages_in_snoozed = Message::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(messages_in_snoozed.len(), 1);
    assert_eq!(messages_in_snoozed[0].local_id, snoozed_message.local_id);
    assert_eq!(messages_in_snoozed[0].snooze_time, snooze_timestamp);

    // Verify the conversation has the snooze time set
    let snoozed_conversation = &conversations_in_snoozed[0];
    let snoozed_label = snoozed_conversation
        .labels
        .iter()
        .find(|label| label.remote_label_id == snoozed.remote_id)
        .expect("Conversation should have snoozed label");
    assert_eq!(snoozed_label.context_snooze_time, snooze_timestamp);

    // Action: Unsnooze the conversation using the Unsnooze action
    let unsnooze_action = proton_mail_common::actions::conversations::Unsnooze::new(
        snoozed.id(),
        vec![conv.local_id.unwrap()],
    );

    user_ctx
        .action_queue()
        .queue_action(unsnooze_action)
        .await
        .unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation:
    // * conversation is no longer in snoozed
    let conversations_in_snoozed_after =
        Conversation::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_snoozed_after.len(), 0);
    let messages_in_snoozed_after = Message::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(messages_in_snoozed_after.len(), 0);

    // * conversation is back in inbox
    let conversations_in_inbox = Conversation::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_inbox.len(), 1);
    let unsnoozed_conversation = &conversations_in_inbox[0];
    assert_eq!(unsnoozed_conversation.id(), conv.local_id.unwrap());

    let messages_in_inbox = Message::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(messages_in_inbox.len(), 1);
    assert_eq!(messages_in_inbox[0].local_id, snoozed_message.local_id);

    // * snooze time is reset to context_time (unsnooze behavior)
    let inbox_label = unsnoozed_conversation
        .labels
        .iter()
        .find(|label| label.remote_label_id == Some(LabelId::inbox()))
        .expect("Conversation should have inbox label");

    assert_eq!(inbox_label.context_snooze_time, inbox_label.context_time);

    // * verify counters are updated correctly
    let snoozed_counters = ConversationCounters::load(snoozed.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snoozed_counters.total, 0);

    let inbox_counters = ConversationCounters::load(inbox.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inbox_counters.total, 1);

    // * message has the correct snooze time
    let unsnoozed_message = &messages_in_inbox[0];
    assert_eq!(unsnoozed_message.snooze_time, UnixTimestamp::new(0));
}

#[tokio::test]
async fn action_unsnooze_with_empty_input_fails() {
    // Test that the Unsnooze action properly handles empty input
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // Get snoozed label
    let snoozed = SystemLabel::Snoozed.load(&tether).await.unwrap().unwrap();

    // Action: Try to unsnooze with empty conversation list
    let unsnooze_action = proton_mail_common::actions::conversations::Unsnooze::new(
        snoozed.id(),
        vec![], // Empty list should cause MailActionError::NoInput
    );

    // This should fail with NoInput error during queueing (failing fast)
    let result = user_ctx.action_queue().queue_action(unsnooze_action).await;

    assert!(result.is_err());

    // Check that it's specifically a NoInput error
    if let Err(error) = result {
        let error_chain = format!("{error:?}");
        assert!(error_chain.contains("NoInput"));
    }
}

#[tokio::test]
async fn snooze_and_unsnooze_are_perfect_counterparts() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    ctx.setup_user(params.clone()).await;

    // Create snooze time (1 hour from now)
    let snooze_time: DateTime<Local> = Local::now() + Duration::hours(1);
    let snooze_timestamp = UnixTimestamp::from(snooze_time);
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let inbox = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();
    let snoozed = SystemLabel::Snoozed.load(&tether).await.unwrap().unwrap();
    let TestData {
        conversation: mut expected_conv,
        label_message: snoozed_message,
        sent_message,
    } = setup_test_label(inbox.id(), &mut tether).await;

    // Snooze the conversation
    tether
        .tx(async |tx| {
            Conversation::snooze(inbox.id(), &[expected_conv.id()], snooze_timestamp, tx).await
        })
        .await
        .unwrap();

    // Unsnooze the conversation
    tether
        .tx(async |tx| Conversation::unsnooze(snoozed.id(), &[expected_conv.id()], tx).await)
        .await
        .unwrap();

    // Remove local ids from labels to make fair comparison
    // ConversationLabels are removed and added back with different local ids
    let mut actual = Conversation::load(expected_conv.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    actual
        .labels
        .iter_mut()
        .for_each(|label| label.local_id = None);
    actual.labels.sort();
    expected_conv
        .labels
        .iter_mut()
        .for_each(|label| label.local_id = None);
    expected_conv.labels.sort();
    pretty_assertions::assert_eq!(expected_conv, actual);

    // Messages are equal
    let actual_message = Message::load(snoozed_message.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    pretty_assertions::assert_eq!(snoozed_message, actual_message);
    let actual_sent_message = Message::load(sent_message.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    pretty_assertions::assert_eq!(sent_message, actual_sent_message);
}
