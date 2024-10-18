use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    ConversationLabel as ApiConversationLabel, Label as ApiLabel, MessageCount as ApiMessageCount,
};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_crypto_account::keys::AddressKeys as ApiAddressKeys;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, Label};
use proton_mail_common::Mailbox;
use proton_mail_test_utils::common::TestContext;
use proton_mail_test_utils::init::Params as TestParams;
use stash::orm::Model;
use velcro::hash_map;

#[tokio::test]
async fn test_label_custom_label() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let (init_params, conv_id, label_id, _) = test_init_params_label();
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(
        &label_id.clone().into(),
        vec![conv_id.clone().into()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &label_id.clone().into(),
        vec![conv_id.clone().into()],
        vec![],
    )
    .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    // Sync the mailbox
    mailbox_inbox.sync(10).await.unwrap();

    let mailbox_label = Mailbox::with_remote_id(user_ctx.clone(), label_id.clone())
        .await
        .expect("failed to create mailbox");

    // Get the conversation id
    let remote_conv_id =
        Conversation::find_first("", vec![], ctx.user_context().await.user_stash())
            .await
            .unwrap()
            .unwrap()
            .remote_id
            .unwrap();

    let label = Label::load(
        mailbox_label.label_id(),
        ctx.user_context().await.user_stash(),
    )
    .await
    .unwrap()
    .unwrap();

    // Label conversation.
    Conversation::apply_label_to_multiple_remote(
        label.remote_id.clone().unwrap(),
        vec![remote_conv_id.clone()],
        None,
        ctx.user_context().await.session().api(),
    )
    .await
    .unwrap();
    // execute the action.
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Unlabel conversation.
    Conversation::remove_label_from_multiple_remote(
        label.remote_id.unwrap(),
        vec![remote_conv_id],
        ctx.user_context().await.session().api(),
    )
    .await
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
    let user_ctx = ctx.user_context().await;

    let (init_params, conv_id, _, _) = test_init_params_label();
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;
    ctx.mock_get_conversations(conversations, 1).await;
    ctx.mock_label_conversation(
        &LabelId::starred().into(),
        vec![conv_id.clone().into()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &LabelId::starred().into(),
        vec![conv_id.clone().into()],
        vec![],
    )
    .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    // Sync the mailbox
    mailbox_inbox.sync(10).await.unwrap();

    let mailbox_label = Mailbox::with_remote_id(user_ctx.clone(), LabelId::starred())
        .await
        .expect("failed to create mailbox");

    // Get the conversation id
    let remote_conv_id =
        Conversation::find_first("", vec![], ctx.user_context().await.user_stash())
            .await
            .unwrap()
            .unwrap()
            .remote_id
            .unwrap();

    let label = Label::load(
        mailbox_label.label_id(),
        ctx.user_context().await.user_stash(),
    )
    .await
    .unwrap()
    .unwrap();

    // Label conversation.
    Conversation::apply_label_to_multiple_remote(
        label.remote_id.clone().unwrap(),
        vec![remote_conv_id.clone()],
        None,
        ctx.user_context().await.session().api(),
    )
    .await
    .unwrap();
    // execute the action.
    user_ctx
        .execute_pending_actions()
        .await
        .expect("failed to flush queue");

    // Unlabel conversation.
    Conversation::remove_label_from_multiple_remote(
        label.remote_id.unwrap(),
        vec![remote_conv_id],
        ctx.user_context().await.session().api(),
    )
    .await
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
    let user_ctx = ctx.user_context().await;

    let (init_params, _, _, folder_id) = test_init_params_label();
    let conversations = init_params.conversations.clone();
    ctx.setup_user(init_params).await;
    ctx.mock_get_conversations(conversations, 1).await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    // Sync the mailbox
    mailbox_inbox.sync(10).await.unwrap();

    let mailbox_folder = Mailbox::with_remote_id(user_ctx.clone(), folder_id.clone())
        .await
        .expect("failed to create mailbox");

    let label = Label::load(
        mailbox_folder.label_id(),
        ctx.user_context().await.user_stash(),
    )
    .await
    .unwrap()
    .unwrap();

    // Get the conversation id
    let remote_conv_id =
        Conversation::find_first("", vec![], ctx.user_context().await.user_stash())
            .await
            .unwrap()
            .unwrap()
            .remote_id
            .unwrap();

    // Label conversation, should fail.
    Conversation::apply_label_to_multiple_remote(
        label.remote_id.unwrap(),
        vec![remote_conv_id],
        None,
        ctx.user_context().await.session().api(),
    )
    .await
    .unwrap_err();
}
fn test_init_params_label() -> (TestParams, RemoteId, LabelId, LabelId) {
    let folder_id = LabelId::from("myfolder");
    let label_id = LabelId::from("mylabel");
    let conv_id = RemoteId::from("conv_id");
    let labels = hash_map! {
        ApiLabelType::Folder: vec![ApiLabel {
            id: folder_id.clone().into(),
            parent_id: None,
            name: "myfolder".to_owned(),
            path: None,
            color: String::new(),
            label_type: ApiLabelType::Folder,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        }],
        ApiLabelType::System: vec![ApiLabel {
            id: LabelId::starred().clone().into(),
            parent_id: None,
            name: "myfolder".to_owned(),
            path: None,
            color: String::new(),
            label_type: ApiLabelType::System,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        }],
        ApiLabelType::Label: vec![ApiLabel {
            id: label_id.clone().into(),
            parent_id: None,
            name: "mylabel".to_owned(),
            path: None,
            color: String::new(),
            label_type: ApiLabelType::Label,
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
            addresses: vec![ApiAddress {
                id: ApiRemoteId::from("myaddress"),
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
                id: conv_id.clone().into(),
                order: 0,
                subject: "Hello".to_owned(),
                senders: vec![],
                recipients: vec![],
                num_messages: 1,
                num_unread: 0,
                num_attachments: 0,
                expiration_time: 0,
                size: 12,
                labels: vec![ApiConversationLabel {
                    id: LabelId::inbox().clone().into(),
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
            conversation_count: vec![ApiConversationCount {
                label_id: LabelId::inbox().clone().into(),
                total: 1,
                unread: 0,
            }],
            message_count: vec![ApiMessageCount {
                label_id: LabelId::inbox().clone().into(),
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
