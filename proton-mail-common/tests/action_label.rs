mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{
    Conversation, ConversationCount, ConversationId, ConversationLabels, Label, LabelId, LabelType,
    MessageCount,
};
use proton_api_mail::exports::crypto::keys::AddressKeys;
use proton_api_mail::proton_api_core::domain::{Address, AddressId, AddressStatus, AddressType};
use proton_mail_common::Mailbox;
use velcro::hash_map;

#[tokio::test]
async fn test_label_custom_label() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context();

    let (init_params, conv_id, label_id, _) = test_init_params_label();
        let conversations = init_params.conversations.clone();
        ctx.setup_user(init_params).await;
        ctx.mock_get_conversations(conversations, 1).await;
        ctx.mock_label_conversation(&label_id, std::iter::once(conv_id.clone()), None, [])
            .await;
        ctx.mock_unlabel_conversation(&label_id, std::iter::once(conv_id.clone()), [])
            .await;
        ctx.catch_all().await;
        let cb = NullCallback {};
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");

    // Sync the mailbox
        mailbox_inbox.sync(10).await.unwrap();

    let mailbox_label =
        Mailbox::with_remote_id(user_ctx.clone(), &label_id).expect("failed to create mailbox");

    // Get the conversation id
    let local_conv_id = mailbox_inbox.conversations(10).unwrap().first().unwrap().id;

    // Label conversation.
    mailbox_inbox
        .label_conversations(mailbox_label.label_id(), std::iter::once(local_conv_id))
        .unwrap();
    // execute the action.
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Unlabel conversation.
    mailbox_inbox
        .unlabel_conversations(mailbox_label.label_id(), std::iter::once(local_conv_id))
        .unwrap();
    // execute the action.
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");
}

#[tokio::test]
async fn test_label_starred() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context();

    let (init_params, conv_id, _, _) = test_init_params_label();
        let conversations = init_params.conversations.clone();
        ctx.setup_user(init_params).await;
        ctx.mock_get_conversations(conversations, 1).await;
        ctx.mock_label_conversation(
            LabelId::starred(),
            std::iter::once(conv_id.clone()),
            None,
            [],
        )
        .await;
        ctx.mock_unlabel_conversation(LabelId::starred(), std::iter::once(conv_id.clone()), [])
            .await;
        ctx.catch_all().await;
        let cb = NullCallback {};
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");

    // Sync the mailbox
        mailbox_inbox.sync(10).await.unwrap();

    let mailbox_label = Mailbox::with_remote_id(user_ctx.clone(), LabelId::starred())
        .expect("failed to create mailbox");

    // Get the conversation id
    let local_conv_id = mailbox_inbox.conversations(10).unwrap().first().unwrap().id;

    // Label conversation.
    mailbox_inbox
        .label_conversations(mailbox_label.label_id(), std::iter::once(local_conv_id))
        .unwrap();
    // execute the action.
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Unlabel conversation.
    mailbox_inbox
        .unlabel_conversations(mailbox_label.label_id(), std::iter::once(local_conv_id))
        .unwrap();
    // execute the action.
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");
}

#[tokio::test]
async fn test_label_fails_when_labelling_folders() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context();

    let (init_params, _, _, folder_id) = test_init_params_label();
        let conversations = init_params.conversations.clone();
        ctx.setup_user(init_params).await;
        ctx.mock_get_conversations(conversations, 1).await;
        ctx.catch_all().await;
        let cb = NullCallback {};
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");

    // Sync the mailbox
        mailbox_inbox.sync(10).await.unwrap();

    let mailbox_folder =
        Mailbox::with_remote_id(user_ctx.clone(), &folder_id).expect("failed to create mailbox");

    // Get the conversation id
    let local_conv_id = mailbox_inbox.conversations(10).unwrap().first().unwrap().id;

    // Label conversation, should fail.
    mailbox_inbox
        .label_conversations(mailbox_folder.label_id(), std::iter::once(local_conv_id))
        .unwrap_err();
}
fn test_init_params_label() -> (TestParams, ConversationId, LabelId, LabelId) {
    let folder_id = LabelId::from("myfolder");
    let label_id = LabelId::from("mylabel");
    let conv_id = ConversationId::from("conv_id");
    let labels = hash_map! {
        LabelType::Folder: vec![Label {
            id: folder_id.clone(),
            parent_id: None,
            name: "myfolder".to_string(),
            path: None,
            color: "".to_string(),
            label_type: LabelType::Folder,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        }],
        LabelType::System: vec![Label {
            id: LabelId::starred().clone(),
            parent_id: None,
            name: "myfolder".to_string(),
            path: None,
            color: "".to_string(),
            label_type: LabelType::System,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        }],
        LabelType::Label: vec![Label {
            id: label_id.clone(),
            parent_id: None,
            name: "mylabel".to_string(),
            path: None,
            color: "".to_string(),
            label_type: LabelType::Label,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        }],
    };
    (
        TestParams {
            last_event_id: None,
            user_info: None,
            user_settings: None,
            mail_settings: None,
            labels,
            addresses: vec![Address {
                id: AddressId::from("myaddress"),
                email: "foo@bar.com".to_string(),
                send: true,
                receive: true,
                status: AddressStatus::Enabled,
                domain_id: None,
                address_type: AddressType::Original,
                order: 0,
                display_name: "".to_string(),
                signature: "".to_string(),
                keys: AddressKeys(vec![]),
                catch_all: false,
                proton_mx: false,
                signed_key_list: Default::default(),
            }],
            conversations: vec![Conversation {
                id: conv_id.clone(),
                order: 0,
                subject: "Hello".to_string(),
                senders: vec![],
                recipients: vec![],
                num_messages: 1,
                num_unread: 0,
                num_attachments: 0,
                expiration_time: 0,
                size: 12,
                labels: vec![ConversationLabels {
                    id: LabelId::inbox().clone(),
                    context_num_unread: 0,
                    context_num_messages: 0,
                    context_time: 0,
                    context_size: 0,
                    context_num_attachments: 0,
                    context_expiration_time: 0,
                    context_snooze_time: 0,
                }],
                display_snooze_reminder: false,
                attachments_metadata: vec![],
                attachment_info: Default::default(),
            }],
            conversation_count: vec![ConversationCount {
                label_id: LabelId::inbox().clone(),
                total: 1,
                unread: 0,
            }],
            message_count: vec![MessageCount {
                label_id: LabelId::inbox().clone(),
                total: 1,
                unread: 0,
            }],
            attachments: Vec::new(),
        },
        conv_id,
        label_id,
        folder_id,
    )
}
