mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{
    Conversation, ConversationCount, ConversationId, ConversationLabels, Label, LabelId, LabelType,
    MessageCount,
};
use proton_api_mail::exports::crypto::domain::AddressKeys;
use proton_api_mail::proton_api_core::domain::{Address, AddressId, AddressStatus, AddressType};
use proton_mail_common::Mailbox;
use std::collections::HashMap;
use velcro::hash_map;

#[test]
fn test_move_between_folders() {
    let ctx = TestContext::new();
    let user_ctx = ctx.user_context();
    let folder_id = LabelId::from("myfolder");
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
    };

    ctx.async_runtime().block_on(async {
        let init_params = test_init_params_folder(&conv_id, labels);
        let conversations = init_params.conversations.clone();
        ctx.setup_user(init_params).await;
        ctx.mock_get_conversations(conversations, 1).await;
        // mock unlabel
        ctx.mock_unlabel_conversation(LabelId::inbox(), [conv_id.clone()], [])
            .await;
        // mock for label
        ctx.mock_label_conversation(&folder_id, [conv_id.clone()], None, [])
            .await;
        // mock unlabel
        ctx.mock_unlabel_conversation(&folder_id, [conv_id.clone()], [])
            .await;
        // mock for label
        ctx.mock_label_conversation(LabelId::inbox(), [conv_id.clone()], None, [])
            .await;
        ctx.catch_all().await;
        let cb = NullCallback {};
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");
    });

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_folder =
        Mailbox::with_remote_id(user_ctx.clone(), &folder_id).expect("failed to create mailbox");

    // Sync the mailbox
    ctx.async_runtime().block_on(async {
        mailbox_inbox.sync(10).await.unwrap();
    });

    // Get the conversation id
    let local_conv_id = mailbox_inbox.conversations(10).unwrap().first().unwrap().id;
    assert!(mailbox_folder.conversations(10).unwrap().is_empty());

    // submit action
    mailbox_inbox
        .move_conversations(mailbox_folder.label_id(), [local_conv_id])
        .expect("failed to move");

    // message should no longer be in inbox and only in the folder
    assert!(mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(!mailbox_folder.conversations(10).unwrap().is_empty());

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(!mailbox_folder.conversations(10).unwrap().is_empty());

    // Move conv back to inbox.
    mailbox_folder
        .move_conversations(mailbox_inbox.label_id(), [local_conv_id])
        .expect("failed to move");

    // message should no longer be in folder and only in the inbox
    assert!(!mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(mailbox_folder.conversations(10).unwrap().is_empty());

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(!mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(mailbox_folder.conversations(10).unwrap().is_empty());
}

#[test]
fn test_move_to_trash_marks_read() {
    let ctx = TestContext::new();
    let user_ctx = ctx.user_context();
    let conv_id = ConversationId::from("conv_id");
    let labels = HashMap::new();

    ctx.async_runtime().block_on(async {
        let init_params = test_init_params_folder(&conv_id, labels);
        let conversations = init_params.conversations.clone();
        ctx.setup_user(init_params).await;
        ctx.mock_get_conversations(conversations, 1).await;
        ctx.mock_unlabel_conversation(LabelId::inbox(), [conv_id.clone()], [])
            .await;
        ctx.mock_label_conversation(LabelId::trash(), [conv_id.clone()], None, [])
            .await;
        ctx.mock_unlabel_conversation(LabelId::trash(), [conv_id.clone()], [])
            .await;
        ctx.mock_mark_conversation_read(std::iter::once(conv_id.clone()), [])
            .await;
        ctx.mock_label_conversation(LabelId::inbox(), [conv_id.clone()], None, [])
            .await;
        ctx.catch_all().await;
        let cb = NullCallback {};
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");
    });

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_trash = Mailbox::with_remote_id(user_ctx.clone(), LabelId::trash())
        .expect("failed to create mailbox");

    // Sync the mailbox
    ctx.async_runtime().block_on(async {
        mailbox_inbox.sync(10).await.unwrap();
    });

    // Get the conversation id
    let local_conv_id = mailbox_inbox.conversations(10).unwrap().first().unwrap().id;
    assert!(mailbox_trash.conversations(10).unwrap().is_empty());

    // submit action
    mailbox_inbox
        .move_conversations(mailbox_trash.label_id(), [local_conv_id])
        .expect("failed to move");

    // message should no longer be in inbox and only in the folder
    assert!(mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(!mailbox_trash.conversations(10).unwrap().is_empty());

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(!mailbox_trash.conversations(10).unwrap().is_empty());

    // Move conv back to inbox, should not mark as unread.
    mailbox_trash
        .move_conversations(mailbox_inbox.label_id(), [local_conv_id])
        .expect("failed to move");

    // message should no longer be in folder and only in the inbox
    assert!(!mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(mailbox_trash.conversations(10).unwrap().is_empty());

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(!mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(mailbox_trash.conversations(10).unwrap().is_empty());
}

#[test]
fn test_move_from_label_does_not_unlabel() {
    let ctx = TestContext::new();
    let user_ctx = ctx.user_context();
    let label_id = LabelId::from("mylabel");
    let conv_id = ConversationId::from("conv_id");
    let labels = hash_map! {
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

    ctx.async_runtime().block_on(async {
        let init_params = test_init_params_label(&conv_id, label_id.clone(), labels);
        let conversations = init_params.conversations.clone();
        ctx.setup_user(init_params).await;
        ctx.mock_get_conversations(conversations, 1).await;
        ctx.mock_label_conversation(LabelId::inbox(), [conv_id.clone()], None, [])
            .await;
        ctx.catch_all().await;
        let cb = NullCallback {};
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");
    });

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_label =
        Mailbox::with_remote_id(user_ctx.clone(), &label_id).expect("failed to create mailbox");

    // Sync the mailbox
    ctx.async_runtime().block_on(async {
        mailbox_inbox.sync(10).await.unwrap();
    });

    // Get the conversation id
    let local_conv_id = mailbox_label.conversations(10).unwrap().first().unwrap().id;
    assert!(mailbox_inbox.conversations(10).unwrap().is_empty());

    // submit action
    mailbox_label
        .move_conversations(mailbox_inbox.label_id(), [local_conv_id])
        .expect("failed to move");

    // message should be in inbox and the label.
    assert!(!mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(!mailbox_label.conversations(10).unwrap().is_empty());

    // flush queue to execute on remote
    // mock for label
    user_ctx
        .execute_pending_actions()
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(!mailbox_inbox.conversations(10).unwrap().is_empty());
    assert!(!mailbox_label.conversations(10).unwrap().is_empty());
}

fn test_init_params_folder(
    conv_id: &ConversationId,
    labels: HashMap<LabelType, Vec<Label>>,
) -> TestParams {
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
                context_num_messages: 1,
                context_time: 0,
                context_size: 12,
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
    }
}
fn test_init_params_label(
    conv_id: &ConversationId,
    label_id: LabelId,
    labels: HashMap<LabelType, Vec<Label>>,
) -> TestParams {
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
                id: label_id,
                context_num_unread: 0,
                context_num_messages: 1,
                context_time: 0,
                context_size: 12,
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
    }
}
