mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{
    ConversationLabels, Label, LabelId, LabelType, Message, MessageFlags, MessageId,
    MessageMetadata, MimeType,
};
use proton_mail_common::Mailbox;

#[test]
fn test_new_mailbox_sync_conversations() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new();
    let mut params = TestParams::default_basic();
    params
        .labels
        .get_mut(&LabelType::Label)
        .unwrap()
        .push(Label {
            id: LabelId::from("testlabel"),
            parent_id: None,
            name: "testlabel".to_string(),
            path: None,
            color: "".to_string(),
            label_type: LabelType::Label,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        });

    let message_id1 = MessageId::from("m1");
    let message_id2 = MessageId::from("m2");

    let messages = vec![
        MessageMetadata {
            id: message_id1.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 0,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox().clone()],
            external_id: None,
            subject: "".to_string(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: MessageFlags::empty(),
            time: 100,
            size: 0,
            unread: false,
            is_replied: false,
            is_replied_all: false,
            is_forwarded: false,
            expiration_time: 0,
            snooze_time: 0,
            num_attachments: 0,
            attachments_metadata: vec![],
        },
        MessageMetadata {
            id: message_id2.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 1,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox().clone()],
            external_id: None,
            subject: "".to_string(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: MessageFlags::empty(),
            time: 200,
            size: 0,
            unread: false,
            is_replied: false,
            is_replied_all: false,
            is_forwarded: false,
            expiration_time: 0,
            snooze_time: 0,
            num_attachments: 0,
            attachments_metadata: vec![],
        },
    ];

    ctx.async_runtime().block_on(async {
        let conversations = params.conversations.clone();
        ctx.setup_user(params.clone()).await;
        ctx.mock_get_conversations(conversations, 1).await;
        ctx.mock_get_conversation_messages(params.conversations[0].clone(), messages, 1)
            .await;
        ctx.catch_all().await;
        ctx.user_context()
            .initialize_async(LabelId::inbox().clone(), &NullCallback {})
            .await
            .expect("failed to initialize");
    });

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(ctx.user_context(), LabelId::inbox()).unwrap();

    // Sync mailbox 1 - this should fire a network request
    ctx.async_runtime().block_on(async {
        mailbox.sync(10).await.unwrap();
    });

    // Get conversations for mailbox.
    let conversations = mailbox.conversations(1).unwrap();

    // Get the message for a conversation.
    let (_, messages) = ctx.async_runtime().block_on(async {
        mailbox
            .conversation_messages(conversations[0].id)
            .await
            .unwrap()
    });

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].rid, Some(message_id1));
    assert_eq!(messages[1].rid, Some(message_id2));

    // Get messages again, but should not fire request.
    let _ = ctx.async_runtime().block_on(async {
        mailbox
            .conversation_messages(conversations[0].id)
            .await
            .unwrap()
    });
}

#[test]
fn test_conversation_sync_from_message_with_remote_id() {
    // check if the conversation is synced from a partial construction when only fetching the
    // messages.

    // Set up a user and initialise the inbox
    let ctx = TestContext::new();
    let mut params = TestParams::default_basic();
    params
        .labels
        .get_mut(&LabelType::Label)
        .unwrap()
        .push(Label {
            id: LabelId::from("testlabel"),
            parent_id: None,
            name: "testlabel".to_string(),
            path: None,
            color: "".to_string(),
            label_type: LabelType::Label,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        });

    let mut conversations = std::mem::take(&mut params.conversations);
    conversations[0].labels.push(ConversationLabels {
        id: LabelId::all_mail().clone(),
        context_num_unread: 0,
        context_num_messages: 1,
        context_time: 0,
        context_size: 0,
        context_num_attachments: 0,
        context_expiration_time: 0,
        context_snooze_time: 0,
    });

    let message_id1 = MessageId::from("m1");

    let message = Message {
        metadata: MessageMetadata {
            id: message_id1.clone(),
            conversation_id: conversations[0].id.clone(),
            order: 0,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox().clone(), LabelId::all_mail().clone()],
            external_id: None,
            subject: "".to_string(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: MessageFlags::empty(),
            time: 100,
            size: 0,
            unread: false,
            is_replied: false,
            is_replied_all: false,
            is_forwarded: false,
            expiration_time: 0,
            snooze_time: 0,
            num_attachments: 0,
            attachments_metadata: vec![],
        },

        header: "".to_owned(),
        parsed_headers: Default::default(),
        body: "hello".to_owned(),
        mime_type: MimeType::TextPlain,
        attachments: vec![],
    };

    let user_context = ctx.user_context();

    ctx.async_runtime().block_on(async {
        ctx.setup_user(params.clone()).await;
        ctx.mock_get_message(&message.metadata.id, message.clone())
            .await;
        ctx.mock_get_conversation(conversations[0].clone(), vec![message.metadata.clone()])
            .await;
        ctx.catch_all().await;
        user_context
            .initialize_async(LabelId::inbox().clone(), &NullCallback {})
            .await
            .expect("failed to initialize");
    });

    let mailbox = Mailbox::with_remote_id(ctx.user_context(), LabelId::inbox()).unwrap();

    // Sync a message
    ctx.async_runtime().block_on(async {
        user_context
            .message_metadata_with_remote_id(&message_id1)
            .await
            .unwrap();
        user_context
            .message_metadata_with_remote_id(&message_id1)
            .await
            .unwrap();
    });

    //Sync conversation
    ctx.async_runtime().block_on(async {
        // First time fetches data
        let conv = user_context
            .conversation_with_remote_id(&conversations[0].id)
            .await
            .unwrap()
            .unwrap();
        // Second load also does nothing
        let _ = user_context
            .conversation_with_remote_id(&conversations[0].id)
            .await
            .unwrap()
            .unwrap();
        // Loading by local id is also a noop at this point.
        user_context
            .conversation_with_id_and_context(conv.id, mailbox.label_id())
            .await
            .unwrap();
    });
}
