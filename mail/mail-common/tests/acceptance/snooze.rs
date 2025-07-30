use chrono::{DateTime, Duration, Local};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{SystemLabel, UnixTimestamp};
use proton_core_common::models::ModelExtension as _;
use proton_mail_common::actions::conversations::Snooze;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, ConversationCounters};
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::scroller::StoreLabeledModelMap as _;
use proton_mail_common::test_utils::test_context::{
    MailTestContext, MailUserContextTestExtension as _,
};
use proton_mail_common::{conv_id, conversation};
use stash::orm::Model;
use stash::stash::StashError;
use velcro::hash_map;

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
    let mut tether = user_ctx.user_stash().connection();

    // Create a conversation in inbox
    let mut conv_data = hash_map! {
        vec![LabelId::inbox()]: vec![conversation!(remote_id: conv_id!("test_conv"))]
    };
    conv_data.save_to_database(&mut tether).await;

    let conv = &conv_data.get(&vec![LabelId::inbox()]).unwrap()[0];

    // Set up counters for inbox
    let inbox = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();
    let mut inbox_conv_counter = ConversationCounters::new(inbox.id());
    inbox_conv_counter.total = 1;

    tether
        .tx::<_, _, StashError>(async |tx| {
            inbox_conv_counter.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Verify initial state - conversation is in inbox
    let conversations_in_inbox = Conversation::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_inbox.len(), 1);
    assert_eq!(conversations_in_inbox[0].id(), conv.local_id.unwrap());

    // Action: Snooze the conversation
    let action = Snooze::new(inbox.id(), vec![conv.local_id.unwrap()], snooze_timestamp);

    user_ctx.action_queue().queue_action(action).await.unwrap();
    user_ctx.execute_single_action().await.unwrap();

    // Validation:
    // * conversation is no longer in inbox
    let conversations_in_inbox_after = Conversation::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_inbox_after.len(), 0);

    // * conversation is now in snoozed
    let snoozed = SystemLabel::Snoozed.load(&tether).await.unwrap().unwrap();
    let conversations_in_snoozed = Conversation::in_label(snoozed.id(), &tether).await.unwrap();

    assert_eq!(conversations_in_snoozed.len(), 1);

    let snoozed_conversation = &conversations_in_snoozed[0];
    assert_eq!(snoozed_conversation.id(), conv.local_id.unwrap());

    // * conversation has the correct snooze time
    let snoozed_label = snoozed_conversation
        .labels
        .iter()
        .find(|label| label.remote_label_id == snoozed.remote_id)
        .expect("Conversation should have snoozed label");

    assert_eq!(snoozed_label.context_snooze_time, snooze_timestamp);
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
    let mut tether = user_ctx.user_stash().connection();

    // Get labels
    let snoozed = SystemLabel::Snoozed.load(&tether).await.unwrap().unwrap();
    let inbox = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();

    // Create a conversation in snoozed with snooze time
    let snooze_time: DateTime<Local> = Local::now() + Duration::hours(1);
    let snooze_timestamp: UnixTimestamp = UnixTimestamp::from(snooze_time);

    let mut conv_data = hash_map! {
        vec![LabelId::snoozed()]: vec![conversation!(remote_id: conv_id!("test_conv"))]
    };
    conv_data.save_to_database(&mut tether).await;

    let conv = &conv_data.get(&vec![LabelId::snoozed()]).unwrap()[0];

    // Set up the conversation with snooze time manually
    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut conversation = Conversation::find_by_id(conv.local_id.unwrap(), tx)
                .await
                .unwrap()
                .unwrap();
            conversation.save(tx).await.unwrap();

            // Set up counters
            let mut snoozed_conv_counter = ConversationCounters::new(snoozed.id());
            snoozed_conv_counter.total = 1;
            snoozed_conv_counter.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Apply snooze manually to set the snooze time
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::snooze(
                inbox.id(),
                vec![conv.local_id.unwrap()],
                snooze_timestamp,
                tx,
            )
            .await
            .unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Verify initial state - conversation is in snoozed
    let conversations_in_snoozed = Conversation::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_snoozed.len(), 1);

    // Action: Unsnooze the conversation
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::unsnooze(snoozed.id(), vec![conv.local_id.unwrap()], tx)
                .await
                .unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Validation:
    // * conversation is no longer in snoozed
    let conversations_in_snoozed_after =
        Conversation::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_snoozed_after.len(), 0);

    // * conversation is back in inbox
    let conversations_in_inbox = Conversation::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_inbox.len(), 1);

    let unsnoozed_conversation = &conversations_in_inbox[0];
    assert_eq!(unsnoozed_conversation.id(), conv.local_id.unwrap());

    // * snooze time is reset to context_time
    let inbox_label = unsnoozed_conversation
        .labels
        .iter()
        .find(|label| label.remote_label_id == Some(LabelId::inbox()))
        .expect("Conversation should have inbox label");

    assert_eq!(inbox_label.context_snooze_time, inbox_label.context_time);
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
    let mut tether = user_ctx.user_stash().connection();

    // Get labels
    let snoozed = SystemLabel::Snoozed.load(&tether).await.unwrap().unwrap();
    let inbox = SystemLabel::Inbox.load(&tether).await.unwrap().unwrap();

    // Create a conversation in inbox (will be snoozed manually in transaction)
    let mut conv_data = hash_map! {
        vec![LabelId::inbox()]: vec![conversation!(remote_id: conv_id!("test_conv"))]
    };
    conv_data.save_to_database(&mut tether).await;

    let conv = &conv_data.get(&vec![LabelId::inbox()]).unwrap()[0];

    // Set up the conversation with snooze time and counters
    tether
        .tx::<_, _, StashError>(async |tx| {
            // Apply snooze to set the snooze time
            Conversation::snooze(
                inbox.id(),
                vec![conv.local_id.unwrap()],
                snooze_timestamp,
                tx,
            )
            .await
            .unwrap();

            // Set up counters for snoozed and inbox
            let mut snoozed_conv_counter = ConversationCounters::new(snoozed.id());
            snoozed_conv_counter.total = 1;
            snoozed_conv_counter.save(tx).await.unwrap();

            // Initialize inbox counters to 0
            let mut inbox_conv_counter = ConversationCounters::new(inbox.id());
            inbox_conv_counter.total = 0;
            inbox_conv_counter.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Verify initial state - conversation is in snoozed with correct snooze time
    let conversations_in_snoozed = Conversation::in_label(snoozed.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_snoozed.len(), 1);
    assert_eq!(conversations_in_snoozed[0].id(), conv.local_id.unwrap());

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

    // * conversation is back in inbox
    let conversations_in_inbox = Conversation::in_label(inbox.id(), &tether).await.unwrap();
    assert_eq!(conversations_in_inbox.len(), 1);

    let unsnoozed_conversation = &conversations_in_inbox[0];
    assert_eq!(unsnoozed_conversation.id(), conv.local_id.unwrap());

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
}

#[tokio::test]
async fn action_unsnooze_with_empty_input_fails() {
    // Test that the Unsnooze action properly handles empty input
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

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
