mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::requests::GetMessagesOptions;
use proton_api_mail::services::proton::response_data::{
    Label as ApiLabel, MessageFlags as ApiMessageFlags, MessageMetadata as ApiMessageMetadata,
};
use proton_core_common::datatypes::LabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, Message};
use proton_mail_common::Mailbox;
use stash::orm::Model;

#[tokio::test]
#[ignore]
async fn test_new_mailbox_sync_conversations() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new().await;
    let mut params = TestParams::default_basic();
    params
        .labels
        .get_mut(&ApiLabelType::Label)
        .unwrap()
        .push(ApiLabel {
            id: ApiRemoteId::from("testlabel"),
            parent_id: None,
            name: "testlabel".to_owned(),
            path: None,
            color: String::new(),
            label_type: ApiLabelType::Label,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        });

    let message_id1 = ApiRemoteId::from("m1");
    let message_id2 = ApiRemoteId::from("m2");

    let messages = vec![
        ApiMessageMetadata {
            id: message_id1.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 0,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox().into()],
            external_id: None,
            subject: String::new(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: ApiMessageFlags::empty(),
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
        ApiMessageMetadata {
            id: message_id2.clone(),
            conversation_id: params.conversations[0].id.clone(),
            order: 1,
            address_id: params.addresses[0].id.clone(),
            label_ids: vec![LabelId::inbox().into()],
            external_id: None,
            subject: String::new(),
            sender: Default::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: ApiMessageFlags::empty(),
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
        .await
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(ctx.user_context().await, LabelId::inbox())
        .await
        .unwrap();

    // Sync mailbox 1 - this should fire a network request
    mailbox.sync(10).await.unwrap();

    // Get conversations for mailbox.
    let conversation = Conversation::find_first("", vec![], ctx.user_context().await.stash())
        .await
        .unwrap()
        .unwrap();

    // Get the message for a conversation.
    let messages = Message::fetch_metadata(
        GetMessagesOptions {
            conversation_id: conversation.remote_id.clone().map(|id| id.into()),
            ..Default::default()
        },
        ctx.user_context().await.session().api(),
    )
    .await
    .unwrap()
    .messages;

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, message_id1);
    assert_eq!(messages[1].id, message_id2);

    // Get messages again, but should not fire request.
    let _ = Message::fetch_metadata(
        GetMessagesOptions {
            conversation_id: conversation.remote_id.map(|id| id.into()),
            ..Default::default()
        },
        ctx.user_context().await.session().api(),
    )
    .await
    .unwrap()
    .messages;
}
