mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{
    Conversation, ConversationCount, ConversationId, ConversationLabels, Label, LabelId, LabelType,
    MessageCount,
};
use proton_api_mail::exports::crypto::keys::AddressKeys;
use proton_api_mail::proton_api_core::domain::{Address, AddressId, AddressStatus, AddressType};
use proton_mail_common::db::LocalConversationId;
use proton_mail_common::Mailbox;
use std::collections::HashMap;
use velcro::hash_map;

#[tokio::test]
async fn test_move_between_folders() {
    let ctx = TestContext::new().await;
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

    let init_params =
        test_init_params_conversation(&conv_id, labels, vec![LabelId::inbox().clone()]);
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;
    ctx.mock_get_conversations(conversations, 1).await;
    ctx.mock_label_conversation(&folder_id, [conv_id.clone()], None, [])
        .await;
    ctx.mock_label_conversation(LabelId::inbox(), [conv_id.clone()], None, [])
        .await;
    ctx.catch_all().await;
    let cb = NullCallback {};
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &cb)
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_folder =
        Mailbox::with_remote_id(user_ctx.clone(), &folder_id).expect("failed to create mailbox");

    // Sync the mailbox
    mailbox_inbox.sync(10).await.unwrap();

    // Get the conversation id
    let local_conv_id = mailbox_inbox.conversations(10).unwrap().first().unwrap().id;
    assert!(!has_conversation(&mailbox_folder, local_conv_id));

    // submit action
    mailbox_inbox
        .move_conversations(mailbox_folder.label_id(), [local_conv_id])
        .expect("failed to move");

    // message should no longer be in inbox and only in the folder
    assert!(!has_conversation(&mailbox_inbox, local_conv_id));
    assert!(has_conversation(&mailbox_folder, local_conv_id));

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(!has_conversation(&mailbox_inbox, local_conv_id));
    assert!(has_conversation(&mailbox_folder, local_conv_id));

    // Move conv back to inbox.
    mailbox_folder
        .move_conversations(mailbox_inbox.label_id(), [local_conv_id])
        .expect("failed to move");

    // message should no longer be in folder and only in the inbox
    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_folder, local_conv_id));

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_folder, local_conv_id));
}

#[tokio::test]
async fn test_move_from_label_does_not_unlabel() {
    let ctx = TestContext::new().await;
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

    let init_params = test_init_params_conversation(&conv_id, labels, vec![label_id.clone()]);
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

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_label =
        Mailbox::with_remote_id(user_ctx.clone(), &label_id).expect("failed to create mailbox");

    // Sync the mailbox
    mailbox_inbox.sync(10).await.unwrap();

    // Get the conversation id
    let local_conv_id = mailbox_label.conversations(10).unwrap().first().unwrap().id;
    assert!(!has_conversation(&mailbox_inbox, local_conv_id));

    // submit action
    mailbox_label
        .move_conversations(mailbox_inbox.label_id(), [local_conv_id])
        .expect("failed to move");

    // message should be in inbox and the label.
    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(has_conversation(&mailbox_label, local_conv_id));

    // flush queue to execute on remote
    // mock for label
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(has_conversation(&mailbox_label, local_conv_id));
}

#[tokio::test]
async fn test_move_into_trash_remove_labels_and_mark_read() {
    // setup
    //   + Create Conversation in inbox with a label

    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context();
    let conv_id = ConversationId::from("conv_id");
    let label_id = LabelId::from("mylabel");
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
        }]
    };

    let init_params = test_init_params_conversation(
        &conv_id,
        labels,
        vec![
            label_id.clone(),
            LabelId::inbox().clone(),
            LabelId::all_mail().clone(),
        ],
    );
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;

    ctx.mock_get_conversations(conversations, 2).await;
    ctx.mock_label_conversation(LabelId::trash(), [conv_id.clone()], None, [])
        .await;
    ctx.mock_label_conversation(LabelId::inbox(), [conv_id.clone()], None, [])
        .await;

    ctx.catch_all().await;
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_trash = Mailbox::with_remote_id(user_ctx.clone(), LabelId::trash())
        .expect("failed to create mailbox");
    let mailbox_all_mail = Mailbox::with_remote_id(user_ctx.clone(), LabelId::all_mail())
        .expect("failed to create mailbox");
    let mailbox_label =
        Mailbox::with_remote_id(user_ctx.clone(), &label_id).expect("failed to create mailbox");

    mailbox_inbox.sync(10).await.expect("failed to sync");
    mailbox_all_mail.sync(10).await.expect("failed to sync");

    let local_conv_id = mailbox_inbox.conversations(10).unwrap().first().unwrap().id;
    assert!(has_conversation(&mailbox_all_mail, local_conv_id));
    assert!(!has_conversation(&mailbox_trash, local_conv_id));
    assert!(has_conversation(&mailbox_label, local_conv_id));

    // actions
    //   + move conversation into trash

    mailbox_inbox
        .move_conversations(mailbox_trash.label_id(), [local_conv_id])
        .expect("failed to move");

    // results
    //   + labels = [ AllMail ]
    //   + conversation marked as read

    assert!(!has_conversation(&mailbox_inbox, local_conv_id));
    assert!(has_conversation(&mailbox_trash, local_conv_id));
    assert!(!has_conversation(&mailbox_label, local_conv_id));
    assert!(has_conversation(&mailbox_all_mail, local_conv_id));

    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    assert!(!has_conversation(&mailbox_inbox, local_conv_id));
    assert!(has_conversation(&mailbox_trash, local_conv_id));
    assert!(!has_conversation(&mailbox_label, local_conv_id));
    assert!(!has_conversation(&mailbox_label, local_conv_id));

    // Move conversation back in Inbox
    //  + conversation should only be in Inbox
    mailbox_trash
        .move_conversations(mailbox_inbox.label_id(), [local_conv_id])
        .expect("failed to move");

    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_trash, local_conv_id));

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_trash, local_conv_id));
}

#[tokio::test]
async fn test_move_into_spam_remove_labels() {
    // setup
    //   + Create Conversation in inbox
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context();
    let conv_id = ConversationId::from("conv_id");
    let label_id = LabelId::from("mylabel");
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
        }]
    };

    let init_params = test_init_params_conversation(
        &conv_id,
        labels,
        vec![
            label_id.clone(),
            LabelId::inbox().clone(),
            LabelId::all_mail().clone(),
        ],
    );
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;

    ctx.mock_get_conversations(conversations, 2).await;
    ctx.mock_label_conversation(LabelId::spam(), [conv_id.clone()], None, [])
        .await;

    ctx.catch_all().await;
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_spam = Mailbox::with_remote_id(user_ctx.clone(), LabelId::spam())
        .expect("failed to create mailbox");
    let mailbox_all_mail = Mailbox::with_remote_id(user_ctx.clone(), LabelId::all_mail())
        .expect("failed to create mailbox");
    let mailbox_label =
        Mailbox::with_remote_id(user_ctx.clone(), &label_id).expect("failed to create mailbox");

    mailbox_inbox.sync(10).await.expect("failed to sync");
    mailbox_all_mail.sync(10).await.expect("failed to sync");

    let local_conv_id = mailbox_inbox.conversations(10).unwrap().first().unwrap().id;
    assert!(!has_conversation(&mailbox_spam, local_conv_id));
    assert!(has_conversation(&mailbox_label, local_conv_id));
    assert!(has_conversation(&mailbox_all_mail, local_conv_id));

    // actions
    //   + move conversation into spam

    mailbox_inbox
        .move_conversations(mailbox_spam.label_id(), [local_conv_id])
        .expect("failed to move");

    // results
    //   + labels = [ AllMail ]

    assert!(!has_conversation(&mailbox_inbox, local_conv_id));
    assert!(has_conversation(&mailbox_spam, local_conv_id));
    assert!(!has_conversation(&mailbox_label, local_conv_id));
    assert!(has_conversation(&mailbox_all_mail, local_conv_id));

    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    assert!(!has_conversation(&mailbox_inbox, local_conv_id));
    assert!(has_conversation(&mailbox_spam, local_conv_id));
    assert!(!has_conversation(&mailbox_label, local_conv_id));
    assert!(has_conversation(&mailbox_all_mail, local_conv_id));
}

#[tokio::test]
async fn move_out_of_trash_set_almost_all_mail() {
    // setup
    //   + Create a Conversation in trash

    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context();
    let conv_id = ConversationId::from("conv_id");

    let init_params =
        test_init_params_conversation(&conv_id, HashMap::new(), vec![LabelId::trash().clone()]);
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;

    ctx.mock_get_conversations(conversations, 3).await;
    ctx.mock_label_conversation(LabelId::inbox(), [conv_id.clone()], None, [])
        .await;

    ctx.catch_all().await;
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_trash = Mailbox::with_remote_id(user_ctx.clone(), LabelId::trash())
        .expect("failed to create mailbox");
    let mailbox_almost_all_mail =
        Mailbox::with_remote_id(user_ctx.clone(), LabelId::almost_all_mail())
            .expect("failed to create mailbox");

    mailbox_trash.sync(10).await.expect("failed to sync");
    mailbox_inbox.sync(10).await.expect("failed to sync");
    mailbox_almost_all_mail
        .sync(10)
        .await
        .expect("failed to sync");

    let local_conv_id = mailbox_trash.conversations(10).unwrap().first().unwrap().id;
    assert!(!has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_almost_all_mail, local_conv_id));

    // actions
    //   + move conversation into inbox

    mailbox_trash
        .move_conversations(mailbox_inbox.label_id(), [local_conv_id])
        .expect("failed to move");

    // results
    //   + conversation in AlmostAllMail

    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_trash, local_conv_id));
    assert!(has_conversation(&mailbox_almost_all_mail, local_conv_id));

    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_trash, local_conv_id));
    assert!(has_conversation(&mailbox_almost_all_mail, local_conv_id));
}

#[tokio::test]
async fn test_move_out_of_spam_set_almost_all_mail() {
    // setup
    //   + Create Conversation in spam

    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context();
    let conv_id = ConversationId::from("conv_id");

    let init_params =
        test_init_params_conversation(&conv_id, HashMap::new(), vec![LabelId::spam().clone()]);
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;

    ctx.mock_get_conversations(conversations, 3).await;
    ctx.mock_label_conversation(LabelId::inbox(), [conv_id.clone()], None, [])
        .await;

    ctx.catch_all().await;
    user_ctx
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .expect("failed to create mailbox");
    let mailbox_spam = Mailbox::with_remote_id(user_ctx.clone(), LabelId::spam())
        .expect("failed to create mailbox");
    let mailbox_almost_all_mail =
        Mailbox::with_remote_id(user_ctx.clone(), LabelId::almost_all_mail())
            .expect("failed to create mailbox");

    mailbox_spam.sync(10).await.expect("failed to sync");
    mailbox_inbox.sync(10).await.expect("failed to sync");
    mailbox_almost_all_mail
        .sync(10)
        .await
        .expect("failed to sync");

    let local_conv_id = mailbox_spam.conversations(10).unwrap().first().unwrap().id;
    assert!(!has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_almost_all_mail, local_conv_id));

    // actions
    //   + move conversation into inbox

    mailbox_spam
        .move_conversations(mailbox_inbox.label_id(), [local_conv_id])
        .expect("failed to move");

    // results
    //   + conversation in AlmostAllMail

    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_spam, local_conv_id));
    assert!(has_conversation(&mailbox_almost_all_mail, local_conv_id));

    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    assert!(has_conversation(&mailbox_inbox, local_conv_id));
    assert!(!has_conversation(&mailbox_spam, local_conv_id));
    assert!(has_conversation(&mailbox_almost_all_mail, local_conv_id));
}

fn has_conversation(mailbox: &Mailbox, local_conversation_id: LocalConversationId) -> bool {
    let conversations = mailbox.conversations(10).unwrap();
    conversations.iter().any(|c| c.id == local_conversation_id)
}

fn test_init_params_conversation(
    conv_id: &ConversationId,
    labels: HashMap<LabelType, Vec<Label>>,
    conversation_labels: Vec<LabelId>,
) -> TestParams {
    let conversation_labels = conversation_labels
        .iter()
        .map(|id| ConversationLabels {
            id: id.clone(),
            context_num_unread: 0,
            context_num_messages: 1,
            context_time: 0,
            context_size: 12,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
        })
        .collect();
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
            labels: conversation_labels,
            display_snooze_reminder: false,
            attachments_metadata: vec![],
            attachment_info: Default::default(),
        }],
        attachments: vec![],
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
