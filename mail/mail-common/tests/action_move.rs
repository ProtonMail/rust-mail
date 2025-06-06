//! These tests are fundamentally broken.
//!
//! All the expectations are built on the `has_conversation` method,
//! but this in turn grabs just the stash from the given mailbox,
//! which ultimately ends up producing assert statements that are
//! self-contradictory.

/*
use proton_core_api::services::proton::{AddressId, LabelId, LabelType as ApiLabelType};
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
    Label as ApiLabel,
};
use proton_mail_api::services::proton::common::ConversationId;
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    ConversationLabel as ApiConversationLabel, MessageCount as ApiMessageCount,
};
use proton_crypto_account::keys::AddressKeys as ApiAddressKeys;
use proton_mail_common::actions::conversations;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Conversation;
use proton_mail_common::Mailbox;
use proton_mail_ids::LocalConversationId;
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::test_context::MailTestContext;
use stash::orm::Model;
use std::collections::HashMap;
use velcro::hash_map;

#[tokio::test]
#[ignore]
async fn test_move_between_folders() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let folder_id = LabelId::from("myfolder");
    let conv_id = ConversationId::from("conv_id");
    let labels = hash_map! {
        ApiLabelType::Folder: vec![ApiLabel {
            id: folder_id.clone(),
            parent_id: None,
            name: "myfolder".to_owned(),
            path: None,
            color: Default::default(),
            label_type: ApiLabelType::Folder,
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
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(&folder_id.clone(), vec![conv_id.clone()], None, vec![])
        .await;
    ctx.mock_label_conversation(&LabelId::inbox(), vec![conv_id.clone()], None, vec![])
        .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");
    let mailbox_folder = Mailbox::with_remote_id(user_ctx.clone(), folder_id.clone())
        .await
        .expect("failed to create mailbox");

    // Sync the mailbox
    mailbox_inbox.sync(10).await.unwrap();

    let tether = user_ctx.user_stash().connection();
    // Get the conversation id
    let local_conv_id = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap()
        .local_id
        .unwrap();
    assert!(!has_conversation(&mailbox_folder, local_conv_id).await);

    // submit action
    user_ctx
        .execute_action(conversations::Move::new(
            mailbox_inbox.label_id(),
            mailbox_folder.label_id(),
            [local_conv_id],
        ))
        .await
        .expect("failed to move");

    // message should no longer be in inbox and only in the folder
    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(has_conversation(&mailbox_folder, local_conv_id).await);

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(has_conversation(&mailbox_folder, local_conv_id).await);

    // Move conv back to inbox.
    user_ctx
        .execute_action(conversations::Move::new(
            mailbox_folder.label_id(),
            mailbox_inbox.label_id(),
            [local_conv_id],
        ))
        .await
        .expect("failed to move");

    // message should no longer be in folder and only in the inbox
    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_folder, local_conv_id).await);

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_folder, local_conv_id).await);
}

#[tokio::test]
#[ignore]
async fn test_move_from_label_does_not_unlabel() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let label_id = LabelId::from("mylabel");
    let conv_id = ConversationId::from("conv_id");
    let labels = hash_map! {
        ApiLabelType::Label: vec![ApiLabel {
            id: label_id.clone(),
            parent_id: None,
            name: "mylabel".to_owned(),
            path: None,
            color: Default::default(),
            label_type: ApiLabelType::Label,
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
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(&LabelId::inbox(), vec![conv_id.clone()], None, vec![])
        .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");
    let mailbox_label = Mailbox::with_remote_id(user_ctx.clone(), label_id.clone())
        .await
        .expect("failed to create mailbox");

    // Sync the mailbox
    mailbox_inbox.sync(10).await.unwrap();
    let tether = user_ctx.user_stash().connection();
    // Get the conversation id
    let local_conv_id = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap()
        .local_id
        .unwrap();
    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);

    // submit action
    user_ctx
        .execute_action(conversations::Move::new(
            mailbox_inbox.label_id(),
            mailbox_label.label_id(),
            [local_conv_id],
        ))
        .await
        .expect("failed to move");

    // message should be in inbox and the label.
    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(has_conversation(&mailbox_label, local_conv_id).await);

    // flush queue to execute on remote
    // mock for label
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(has_conversation(&mailbox_label, local_conv_id).await);
}

#[tokio::test]
#[ignore]
async fn test_move_into_trash_remove_labels_and_mark_read() {
    // setup
    //   + Create Conversation in inbox with a label

    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let conv_id = ConversationId::from("conv_id");
    let label_id = LabelId::from("mylabel");
    let labels = hash_map! {
        ApiLabelType::Label: vec![ApiLabel {
            id: label_id.clone(),
            parent_id: None,
            name: "mylabel".to_owned(),
            path: None,
            color: Default::default(),
            label_type: ApiLabelType::Label,
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

    ctx.mock_get_conversations(conversations, 2_u64).await;
    ctx.mock_label_conversation(&LabelId::trash(), vec![conv_id.clone()], None, vec![])
        .await;
    ctx.mock_label_conversation(&LabelId::inbox(), vec![conv_id.clone()], None, vec![])
        .await;

    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");
    let mailbox_trash = Mailbox::with_remote_id(user_ctx.clone(), LabelId::trash())
        .await
        .expect("failed to create mailbox");
    let mailbox_all_mail = Mailbox::with_remote_id(user_ctx.clone(), LabelId::all_mail())
        .await
        .expect("failed to create mailbox");
    let mailbox_label = Mailbox::with_remote_id(user_ctx.clone(), label_id.clone())
        .await
        .expect("failed to create mailbox");

    mailbox_inbox.sync(10).await.expect("failed to sync");
    mailbox_all_mail.sync(10).await.expect("failed to sync");
    let tether = user_ctx.user_stash().connection();
    let local_conv_id = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap()
        .local_id
        .unwrap();
    assert!(has_conversation(&mailbox_all_mail, local_conv_id).await);
    assert!(!has_conversation(&mailbox_trash, local_conv_id).await);
    assert!(has_conversation(&mailbox_label, local_conv_id).await);

    // actions
    //   + move conversation into trash

    user_ctx
        .execute_action(conversations::Move::new(
            mailbox_inbox.label_id(),
            mailbox_trash.label_id(),
            [local_conv_id],
        ))
        .await
        .expect("failed to move");

    // results
    //   + labels = [ AllMail ]
    //   + conversation marked as read

    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(has_conversation(&mailbox_trash, local_conv_id).await);
    assert!(!has_conversation(&mailbox_label, local_conv_id).await);
    assert!(has_conversation(&mailbox_all_mail, local_conv_id).await);

    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(has_conversation(&mailbox_trash, local_conv_id).await);
    assert!(!has_conversation(&mailbox_label, local_conv_id).await);
    assert!(!has_conversation(&mailbox_label, local_conv_id).await);

    // Move conversation back in Inbox
    //  + conversation should only be in Inbox
    user_ctx
        .execute_action(conversations::Move::new(
            mailbox_trash.label_id(),
            mailbox_inbox.label_id(),
            [local_conv_id],
        ))
        .await
        .expect("failed to move");

    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_trash, local_conv_id).await);

    // flush queue to execute on remote
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Note, there is no way to validate action got successfully executed, have to check locally
    // if the messages are in the right place again.
    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_trash, local_conv_id).await);
}

#[tokio::test]
#[ignore]
async fn test_move_into_spam_remove_labels() {
    // setup
    //   + Create Conversation in inbox
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let conv_id = ConversationId::from("conv_id");
    let label_id = LabelId::from("mylabel");
    let labels = hash_map! {
        ApiLabelType::Label: vec![ApiLabel {
            id: label_id.clone(),
            parent_id: None,
            name: "mylabel".to_owned(),
            path: None,
            color: Default::default(),
            label_type: ApiLabelType::Label,
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

    ctx.mock_get_conversations(conversations, 2_u64).await;
    ctx.mock_label_conversation(&LabelId::spam(), vec![conv_id.clone()], None, vec![])
        .await;

    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");
    let mailbox_spam = Mailbox::with_remote_id(user_ctx.clone(), LabelId::spam())
        .await
        .expect("failed to create mailbox");
    let mailbox_all_mail = Mailbox::with_remote_id(user_ctx.clone(), LabelId::all_mail())
        .await
        .expect("failed to create mailbox");
    let mailbox_label = Mailbox::with_remote_id(user_ctx.clone(), label_id.clone())
        .await
        .expect("failed to create mailbox");

    mailbox_inbox.sync(10).await.expect("failed to sync");
    mailbox_all_mail.sync(10).await.expect("failed to sync");
    let tether = user_ctx.user_stash().connection();
    let local_conv_id = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap()
        .local_id
        .unwrap();
    assert!(!has_conversation(&mailbox_spam, local_conv_id).await);
    assert!(has_conversation(&mailbox_label, local_conv_id).await);
    assert!(has_conversation(&mailbox_all_mail, local_conv_id).await);

    // actions
    //   + move conversation into spam

    user_ctx
        .execute_action(conversations::Move::new(
            mailbox_inbox.label_id(),
            mailbox_spam.label_id(),
            [local_conv_id],
        ))
        .await
        .expect("failed to move");

    // results
    //   + labels = [ AllMail ]

    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(has_conversation(&mailbox_spam, local_conv_id).await);
    assert!(!has_conversation(&mailbox_label, local_conv_id).await);
    assert!(has_conversation(&mailbox_all_mail, local_conv_id).await);

    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(has_conversation(&mailbox_spam, local_conv_id).await);
    assert!(!has_conversation(&mailbox_label, local_conv_id).await);
    assert!(has_conversation(&mailbox_all_mail, local_conv_id).await);
}

#[tokio::test]
#[ignore]
async fn move_out_of_trash_set_almost_all_mail() {
    // setup
    //   + Create a Conversation in trash

    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let conv_id = ConversationId::from("conv_id");

    let init_params =
        test_init_params_conversation(&conv_id, HashMap::new(), vec![LabelId::trash().clone()]);
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;

    ctx.mock_get_conversations(conversations, 3_u64).await;
    ctx.mock_label_conversation(&LabelId::inbox(), vec![conv_id.clone()], None, vec![])
        .await;

    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");
    let mailbox_trash = Mailbox::with_remote_id(user_ctx.clone(), LabelId::trash())
        .await
        .expect("failed to create mailbox");
    let mailbox_almost_all_mail =
        Mailbox::with_remote_id(user_ctx.clone(), LabelId::almost_all_mail())
            .await
            .expect("failed to create mailbox");

    mailbox_trash.sync(10).await.expect("failed to sync");
    mailbox_inbox.sync(10).await.expect("failed to sync");
    mailbox_almost_all_mail
        .sync(10)
        .await
        .expect("failed to sync");
    let tether = user_ctx.user_stash().connection();
    let local_conv_id = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap()
        .local_id
        .unwrap();
    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_almost_all_mail, local_conv_id).await);

    // actions
    //   + move conversation into inbox
    user_ctx
        .execute_action(conversations::Move::new(
            mailbox_trash.label_id(),
            mailbox_inbox.label_id(),
            [local_conv_id],
        ))
        .await
        .expect("failed to move");

    // results
    //   + conversation in AlmostAllMail

    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_trash, local_conv_id).await);
    assert!(has_conversation(&mailbox_almost_all_mail, local_conv_id).await);

    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_trash, local_conv_id).await);
    assert!(has_conversation(&mailbox_almost_all_mail, local_conv_id).await);
}

#[tokio::test]
#[ignore]
async fn test_move_out_of_spam_set_almost_all_mail() {
    // setup
    //   + Create Conversation in spam

    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let conv_id = ConversationId::from("conv_id");

    let init_params =
        test_init_params_conversation(&conv_id, HashMap::new(), vec![LabelId::spam().clone()]);
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;

    ctx.mock_get_conversations(conversations, 3_u64).await;
    ctx.mock_label_conversation(&LabelId::inbox(), vec![conv_id.clone()], None, vec![])
        .await;

    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");
    let mailbox_spam = Mailbox::with_remote_id(user_ctx.clone(), LabelId::spam())
        .await
        .expect("failed to create mailbox");
    let mailbox_almost_all_mail =
        Mailbox::with_remote_id(user_ctx.clone(), LabelId::almost_all_mail())
            .await
            .expect("failed to create mailbox");

    mailbox_spam.sync(10).await.expect("failed to sync");
    mailbox_inbox.sync(10).await.expect("failed to sync");
    mailbox_almost_all_mail
        .sync(10)
        .await
        .expect("failed to sync");
    let tether = user_ctx.user_stash().connection();
    let local_conv_id = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap()
        .local_id
        .unwrap();
    assert!(!has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_almost_all_mail, local_conv_id).await);

    // actions
    //   + move conversation into inbox

    user_ctx
        .execute_action(conversations::Move::new(
            mailbox_spam.label_id(),
            mailbox_inbox.label_id(),
            [local_conv_id],
        ))
        .await
        .expect("failed to move");

    // results
    //   + conversation in AlmostAllMail

    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_spam, local_conv_id).await);
    assert!(has_conversation(&mailbox_almost_all_mail, local_conv_id).await);

    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    assert!(has_conversation(&mailbox_inbox, local_conv_id).await);
    assert!(!has_conversation(&mailbox_spam, local_conv_id).await);
    assert!(has_conversation(&mailbox_almost_all_mail, local_conv_id).await);
}

async fn has_conversation(mailbox: &Mailbox, local_conversation_id: LocalConversationId) -> bool {
    let tether = mailbox.stash().connection();
    let conversations = Conversation::find_first("", vec![], &tether).await.unwrap();
    conversations
        .iter()
        .any(|c| c.id() == local_conversation_id)
}

fn test_init_params_conversation(
    conv_id: &ConversationId,
    labels: HashMap<ApiLabelType, Vec<ApiLabel>>,
    conversation_labels: Vec<LabelId>,
) -> TestParams {
    let conversation_labels = conversation_labels
        .iter()
        .map(|id| ApiConversationLabel {
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
        labels,
        addresses: vec![ApiAddress {
            id: AddressId::from("myaddress"),
            email: "foo@bar.com".to_owned(),
            send: true,
            receive: true,
            status: ApiAddressStatus::Enabled,
            domain_id: None,
            address_type: ApiAddressType::Original,
            order: 0,
            display_name: String::new(),
            signature: String::new(),
            keys: ApiAddressKeys(vec![]),
            catch_all: false,
            proton_mx: false,
            signed_key_list: Default::default(),
        }],
        conversations: vec![ApiConversation {
            id: conv_id.clone(),
            order: 0,
            subject: "Hello".to_owned(),
            senders: vec![],
            recipients: vec![],
            num_messages: 1,
            num_unread: 0,
            num_attachments: 0,
            expiration_time: 0,
            size: 12,
            labels: conversation_labels,
            ..Default::default()
        }],
        conversation_count: vec![ApiConversationCount {
            label_id: LabelId::inbox(),
            total: 1,
            unread: 0,
        }],
        message_count: vec![ApiMessageCount {
            label_id: LabelId::inbox(),
            total: 1,
            unread: 0,
        }],
        ..Default::default()
    }
}
*/
