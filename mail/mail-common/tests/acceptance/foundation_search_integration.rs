use proton_core_api::services::proton::prelude::{Action, EventId};
use proton_core_common::datatypes::SystemLabel;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::response_data::{MailEvent, MessageEvent};
use proton_mail_common::api_message_meta;
use proton_mail_common::models::Message;
use proton_mail_common::test_utils::{
    init::Params as TestParams,
    test_context::{MailTestContext, MailUserContextTestExtension},
};
use proton_mail_search::{SearchIndexIntent, SearchOperation};
use stash::orm::Model;

/// Integration test for Foundation Search indexing via message events
///
/// This test exercises the event-to-intent flow:
/// 1. Create messages via events (triggers search indexing inline in transaction)
/// 2. Verify messages were persisted
/// 3. Verify search indexing intents were queued with correct operations and message IDs
///
/// The actual indexing, search, and scroller integration are tested separately.
#[tokio::test]
async fn test_search_indexing_intents_queued_on_message_events() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();

    // Create messages with searchable content
    let message1 = api_message_meta!(
        id: MessageId::from("msg1"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::AllMail.remote_id()],
        subject: "Project timeline discussion".to_string()
    );

    let message2 = api_message_meta!(
        id: MessageId::from("msg2"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::AllMail.remote_id()],
        subject: "Quarterly report review".to_string()
    );

    let message3 = api_message_meta!(
        id: MessageId::from("msg3"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::AllMail.remote_id()],
        subject: "Budget meeting scheduled".to_string()
    );

    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;

    // Step 1: Create messages via events (this triggers search indexing inline)
    user_ctx
        .apply_event(MailEvent {
            event_id: EventId::from("event1"),
            labels: None,
            conversation_counts: None,
            conversations: None,
            incoming_defaults: None,
            mail_settings: None,
            message_counts: None,
            messages: Some(vec![
                MessageEvent {
                    id: message1.id.clone(),
                    action: Action::Create,
                    message: Some(message1.clone()),
                },
                MessageEvent {
                    id: message2.id.clone(),
                    action: Action::Create,
                    message: Some(message2.clone()),
                },
                MessageEvent {
                    id: message3.id.clone(),
                    action: Action::Create,
                    message: Some(message3.clone()),
                },
            ]),
            refresh: 0,
            has_more: false,
        })
        .await
        .unwrap();

    // Step 2: Verify messages were created and collect their local IDs
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let msg1 = Message::find_by_remote_id(message1.id.clone(), &tether)
        .await
        .unwrap()
        .expect("Message 1 should exist");
    let msg2 = Message::find_by_remote_id(message2.id.clone(), &tether)
        .await
        .unwrap()
        .expect("Message 2 should exist");
    let msg3 = Message::find_by_remote_id(message3.id.clone(), &tether)
        .await
        .unwrap()
        .expect("Message 3 should exist");

    let expected_local_ids: Vec<u64> =
        vec![msg1.id().as_u64(), msg2.id().as_u64(), msg3.id().as_u64()];

    // Step 3: Verify search indexing intents were queued correctly
    let intents = SearchIndexIntent::get_pending_batch(&tether, 10)
        .await
        .unwrap();

    assert_eq!(
        intents.len(),
        3,
        "Should have 3 search indexing intents queued after message creation"
    );

    // Verify all intents are Index operations
    for intent in &intents {
        assert_eq!(
            intent.operation,
            SearchOperation::Index,
            "All intents from Create events should be Index operations"
        );
    }

    // Verify the intent message IDs match the created messages' local IDs
    let intent_message_ids: Vec<u64> = intents.iter().map(|i| i.message_id).collect();
    for expected_id in &expected_local_ids {
        assert!(
            intent_message_ids.contains(expected_id),
            "Intent for local message ID {} should exist",
            expected_id
        );
    }
}
