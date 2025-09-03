use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::{LabelId, LabelType as ApiLabelType};
use proton_core_common::models::ModelExtension;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::response_data::ConversationLabel as ApiConversationLabel;
use proton_mail_api::services::proton::response_data::MessageMetadata as ApiMessageMetadata;
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::{ContextualConversation, SystemLabelId};
use proton_mail_common::models::Conversation;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use stash::orm::Model;

#[tokio::test]
async fn test_new_mailbox_sync_conversations() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::new().await;
    let mut params = TestParams::default_basic();
    params
        .labels
        .get_mut(&ApiLabelType::Label)
        .unwrap()
        .push(ApiLabel {
            id: LabelId::from("testlabel"),
            name: "testlabel".to_owned(),
            label_type: ApiLabelType::Label,
            ..ApiLabel::test_default()
        });

    let message_id1 = MessageId::from("m1");
    let message_id2 = MessageId::from("m2");

    let messages = vec![
        ApiMessageMetadata {
            id: message_id1.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 0,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox()],
            ..ApiMessageMetadata::test_default()
        },
        ApiMessageMetadata {
            id: message_id2.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 1,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox()],
            ..ApiMessageMetadata::test_default()
        },
    ];

    let conversations = params.conversations.clone();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_get_conversation_messages(params.conversations[0].clone(), messages, 1_u64)
        .await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();

    // Sync mailbox 1 - this should fire a network request
    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();
    let tether = user_ctx.user_stash().connection().await.unwrap();
    // Get conversations for mailbox.
    let conversation = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap();

    // Get the message for a conversation.

    let result = ContextualConversation::conversation_and_messages(
        user_ctx.network_monitor_service(),
        conversation.id(),
        mailbox.label_id(),
        user_ctx.user_stash(),
        user_ctx.session(),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(result.messages.len(), 2);
    assert_eq!(result.messages[0].remote_id, Some(message_id1));
    assert_eq!(result.messages[1].remote_id, Some(message_id2));

    // Get messages again, but should not fire request.
    let _ = ContextualConversation::conversation_and_messages(
        user_ctx.network_monitor_service(),
        conversation.id(),
        mailbox.label_id(),
        user_ctx.user_stash(),
        user_ctx.session(),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn test_new_mailbox_syncs_new_conversation_messages_on_push_notification() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::new().await;
    let mut params = TestParams::default_basic();
    let new_label_id = LabelId::from("NEW_LABEL");
    {
        let labels = params.labels.get_mut(&ApiLabelType::Label).unwrap();

        labels.push(ApiLabel {
            id: LabelId::from("testlabel"),
            name: "testlabel".to_owned(),
            label_type: ApiLabelType::Label,
            ..ApiLabel::test_default()
        });

        labels.push(ApiLabel {
            id: new_label_id.clone(),
            name: "testlabel2".to_owned(),
            label_type: ApiLabelType::Label,
            ..ApiLabel::test_default()
        });
    }

    let message_id1 = MessageId::from("m1");
    let message_id2 = MessageId::from("m2");
    let message_id3 = MessageId::from("m3");

    let messages = vec![
        ApiMessageMetadata {
            id: message_id1.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 0,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox()],
            ..ApiMessageMetadata::test_default()
        },
        ApiMessageMetadata {
            id: message_id2.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 1,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox()],
            ..ApiMessageMetadata::test_default()
        },
    ];

    let messages_updated = vec![
        ApiMessageMetadata {
            id: message_id1.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 0,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox()],
            ..ApiMessageMetadata::test_default()
        },
        ApiMessageMetadata {
            id: message_id2.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 1,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox()],
            ..ApiMessageMetadata::test_default()
        },
        ApiMessageMetadata {
            id: message_id3.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 2,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox()],
            ..ApiMessageMetadata::test_default()
        },
    ];

    let mut conv_updated = params.conversations[0].clone();
    conv_updated.labels.push(ApiConversationLabel {
        id: new_label_id.clone(),
        context_expiration_time: 0,
        context_num_attachments: 0,
        context_num_messages: 20,
        context_num_unread: 0,
        context_size: 0,
        context_snooze_time: 0,
        context_time: 0,
    });

    let conversations = params.conversations.clone();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_get_conversation_messages(params.conversations[0].clone(), messages, 1_u64)
        .await;
    let user_ctx = ctx.mail_user_context().await;

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();

    // Sync mailbox 1 - this should fire a network request
    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();
    let tether = user_ctx.user_stash().connection().await.unwrap();
    // Get conversations for mailbox.
    let conversation = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap();

    // Get the message for a conversation.

    let result = ContextualConversation::conversation_and_messages(
        user_ctx.network_monitor_service(),
        conversation.id(),
        mailbox.label_id(),
        user_ctx.user_stash(),
        user_ctx.session(),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(result.messages.len(), 2);
    assert_eq!(result.messages[0].remote_id.as_ref(), Some(&message_id1));
    assert_eq!(result.messages[1].remote_id.as_ref(), Some(&message_id2));

    ctx.mock_server().reset().await;
    ctx.mock_get_conversation_messages(conv_updated, messages_updated, 1_u64)
        .await;
    ctx.catch_all().await;
    // Get messages again, should have new message
    let result = ContextualConversation::conversation_and_messages_from_push_notification(
        user_ctx.network_monitor_service(),
        conversation.id(),
        mailbox.label_id(),
        user_ctx.user_stash(),
        user_ctx.session(),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(result.messages.len(), 3);
    assert_eq!(result.messages[0].remote_id.as_ref(), Some(&message_id1));
    assert_eq!(result.messages[1].remote_id.as_ref(), Some(&message_id2));
    assert_eq!(result.messages[2].remote_id.as_ref(), Some(&message_id3));

    let conv = Conversation::find_by_id(
        result.conversation.local_id,
        &user_ctx.user_stash().connection().await.unwrap(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        conv.labels
            .iter()
            .any(|l| l.remote_label_id.as_ref() == Some(&new_label_id))
    );
}

// #[test]
// fn test_conversation_sync_from_message_with_remote_id() {
//     // check if the conversation is synced from a partial construction when only fetching the
//     // messages.
//
//     // Set up a user and initialise the inbox
//     let ctx = MailTestContext::new();
//     let mut params = TestParams::default_basic();
//     params
//         .labels
//         .get_mut(&LabelType::Label)
//         .unwrap()
//         .push(Label {
//             id: LabelId::from("testlabel"),
//             parent_id: None,
//             name: "testlabel".to_string(),
//             path: None,
//             color: "".to_string(),
//             label_type: LabelType::Label,
//             notify: false,
//             display: false,
//             sticky: false,
//             expanded: false,
//             order: 0,
//         });
//
//     let mut conversations = std::mem::take(&mut params.conversations);
//     conversations[0].labels.push(ConversationLabels {
//         id: LabelId::all_mail().clone(),
//         context_num_unread: 0,
//         context_num_messages: 1,
//         context_time: 0,
//         context_size: 0,
//         context_num_attachments: 0,
//         context_expiration_time: 0,
//         context_snooze_time: 0,
//     });
//
//     let message_id1 = MessageId::from("m1");
//
//     let message = Message {
//         metadata: MessageMetadata {
//             id: message_id1.clone(),
//             conversation_id: conversations[0].id.clone(),
//             order: 0,
//             address_id: params.addresses[0].id.clone(),
//             label_ids: vec![LabelId::inbox().clone(), LabelId::all_mail().clone()],
//             external_id: None,
//             subject: "".to_string(),
//             sender: Default::default(),
//             to_list: vec![],
//             cc_list: vec![],
//             bcc_list: vec![],
//             reply_tos: vec![],
//             flags: MessageFlags::empty(),
//             time: 100,
//             size: 0,
//             unread: false,
//             is_replied: false,
//             is_replied_all: false,
//             is_forwarded: false,
//             expiration_time: 0,
//             snooze_time: 0,
//             num_attachments: 0,
//             attachments_metadata: vec![],
//         },
//
//         header: "".to_owned(),
//         parsed_headers: Default::default(),
//         body: "hello".to_owned(),
//         mime_type: MimeType::TextPlain,
//         attachments: vec![],
//     };
//
//     let user_context = ctx.mail_user_context();
//
//     ctx.async_runtime().block_on(async {
//         ctx.setup_user(params.clone()).await;
//         ctx.mock_get_message(&message.metadata.id, message.clone())
//             .await;
//         ctx.mock_get_conversation(conversations[0].clone(), vec![message.metadata.clone()])
//             .await;
//         ctx.catch_all().await;
//         user_context
//             .initialize_async(LabelId::inbox().clone(), &NullCallback {})
//             .await
//             .expect("failed to initialize");
//     });
//
//     let mailbox = Mailbox::with_remote_id(ctx.mail_user_context(), LabelId::inbox()).unwrap();
//
//     // Sync a message
//     ctx.async_runtime().block_on(async {
//         user_context
//             .message_metadata_with_remote_id(&message_id1)
//             .await
//             .unwrap();
//         user_context
//             .message_metadata_with_remote_id(&message_id1)
//             .await
//             .unwrap();
//     });
//
//     //Sync conversation
//     ctx.async_runtime().block_on(async {
//         // First time fetches data
//         let conv = user_context
//             .conversation_with_remote_id(&conversations[0].id)
//             .await
//             .unwrap()
//             .unwrap();
//         // Second load also does nothing
//         let _ = user_context
//             .conversation_with_remote_id(&conversations[0].id)
//             .await
//             .unwrap()
//             .unwrap();
//         // Loading by local id is also a noop at this point.
//         user_context
//             .conversation_with_id_and_context(conv.id, mailbox.label_id())
//             .await
//             .unwrap();
//     });
// }
