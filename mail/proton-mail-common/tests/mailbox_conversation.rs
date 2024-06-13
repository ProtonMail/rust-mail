mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{
    Label, LabelId, LabelType, MessageFlags, MessageId, MessageMetadata,
};
use proton_mail_common::Mailbox;

#[tokio::test]
async fn test_new_mailbox_sync_conversations() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new().await;
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

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(ctx.user_context(), LabelId::inbox()).unwrap();

    // Sync mailbox 1 - this should fire a network request
    mailbox.sync(10).await.unwrap();

    // Get conversations for mailbox.
    let conversations = mailbox.conversations(1).unwrap();

    // Get the message for a conversation.
    let (_, messages) = mailbox
        .conversation_messages(conversations[0].id)
        .await
        .unwrap();

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].rid, Some(message_id1));
    assert_eq!(messages[1].rid, Some(message_id2));

    // Get messages again, but should not fire request.
    let _ = mailbox
        .conversation_messages(conversations[0].id)
        .await
        .unwrap();
}
