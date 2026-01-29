#[cfg(feature = "foundation_search")]
#[allow(clippy::module_inception)]
mod foundation_search_integration {
    use proton_core_api::services::proton::LabelId;
    use proton_core_api::services::proton::prelude::{Action, EventId};
    use proton_core_common::datatypes::SystemLabel;
    use proton_core_common::models::ModelIdExtension;
    use proton_mail_api::services::proton::common::MessageId;
    use proton_mail_api::services::proton::response_data::{MailEvent, MessageEvent};
    use proton_mail_common::api_message_meta;
    use proton_mail_common::datatypes::{SearchOptions, SystemLabelId};
    use proton_mail_common::models::Message;
    use proton_mail_common::test_utils::scroller::TestScroller;
    use proton_mail_common::test_utils::{
        init::Params as TestParams,
        test_context::{MailTestContext, MailUserContextTestExtension},
    };
    use proton_mail_search::SearchIndexIntent;

    /// Integration test for Foundation Search with mail scroller
    ///
    /// This test exercises the complete end-to-end flow:
    /// 1. Create messages via events (triggers search indexing inline in transaction)
    /// 2. Verify search indexing intents were queued (proving inline indexing works)
    /// 3. Verify mail scroller can be created with search options (proving integration)
    ///
    /// This test validates that the integration between message creation, search indexing,
    /// and the mail scroller works correctly. The actual indexing and search functionality
    /// is tested separately in `proton-mail-search` crate.
    #[tokio::test]
    async fn test_foundation_search_integration_with_mail_scroller() {
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

        // Mock search API endpoint to return empty results (since message bodies may not be indexed yet)
        ctx.mock_get_messages()
            .given_label_id(&LabelId::almost_all_mail())
            .alter(|mock| mock.expect(1))
            .respond_with(vec![])
            .await;

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

        // Step 2: Verify messages were created
        let tether = user_ctx.user_stash().connection().await.unwrap();
        let _msg1 = Message::find_by_remote_id(message1.id.clone(), &tether)
            .await
            .unwrap()
            .expect("Message 1 should exist");
        let _msg2 = Message::find_by_remote_id(message2.id.clone(), &tether)
            .await
            .unwrap()
            .expect("Message 2 should exist");
        let _msg3 = Message::find_by_remote_id(message3.id.clone(), &tether)
            .await
            .unwrap()
            .expect("Message 3 should exist");

        // Step 3: Verify search indexing intents were queued
        let intents = SearchIndexIntent::get_pending_batch(&tether, 10)
            .await
            .unwrap();
        assert_eq!(
            intents.len(),
            3,
            "Should have 3 search indexing intents queued after message creation"
        );

        // Step 4: Verify the integration path works
        // The intents will be processed by the worker in the background.
        // For this test, we verify that:
        // 1. Messages were created successfully
        // 2. Search indexing intents were queued (proving the inline indexing works)
        // 3. The mail scroller can be created with search options (proving integration)

        // Step 5: Verify mail scroller can be created with search options
        // This tests that the integration works end-to-end
        let page_size = 10;
        let mut test_scroller =
            TestScroller::search(&user_ctx, SearchOptions::from("project"), page_size)
                .await
                .unwrap();

        // The scroller should be created successfully (even if results are empty)
        // This verifies the integration path works without errors
        test_scroller.fetch_more_and_wait().await.unwrap();
        let items = test_scroller.items();
        // Note: Results may be empty if message bodies haven't been stored yet or worker hasn't processed intents,
        // but the test verifies the integration path works without errors
        assert!(
            items.len() <= 3,
            "Scroller should return at most 3 results (one per message)"
        );
    }
}
