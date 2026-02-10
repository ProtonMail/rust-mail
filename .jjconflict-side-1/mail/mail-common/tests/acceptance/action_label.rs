use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType as ApiLabelType;
use proton_core_api::services::proton::{Address as ApiAddress, Label as ApiLabel};
use proton_core_common::datatypes::LabelType;
use proton_core_common::models::{Address, Label, ModelIdExtension};
use proton_core_common::test_utils::addresses::ApiAddressTestUtils;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    MessageCount as ApiMessageCount,
};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, Message};
use proton_mail_common::test_utils::conversations::ApiConversationTestUtils;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::labels::ApiLabelTestUtils;
use proton_mail_common::test_utils::mailbox::MailboxTestUtils;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use std::collections::HashMap;
use velcro::hash_map;

/// Validates that it is possible to apply custom label to conversations located in Inbox folder.
#[tokio::test]
async fn test_labeling_conversation_with_custom_label() {
    // General setup
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Set up test data
    let remote_label_name = "selected";
    let (remote_label, remote_label_id) =
        ApiLabel::create_api_label(remote_label_name, LabelType::Label);
    let remote_labels = hash_map! {
        ApiLabelType::Label: vec![remote_label.clone()],
    };
    let inbox_mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();
    let inbox_remote_label = ApiLabel::get_api_label_with_given_id(LabelId::inbox());
    let inbox_local_label = inbox_mailbox.get_local_label(&tether).await;

    let remote_conversation_id = "first";
    let remote_conversation =
        ApiConversation::test_conversation(remote_conversation_id, vec![inbox_remote_label]);
    let conversations = vec![remote_conversation.clone()];

    let params = test_init_params(remote_labels, conversations.clone());

    // Set up mocks
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(
        &remote_label_id,
        vec![remote_conversation.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &remote_label_id,
        vec![remote_conversation.id.clone()],
        vec![],
    )
    .await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    inbox_mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let local_conversation = Conversation::load(1.into(), &tether)
        .await
        .unwrap()
        .unwrap();
    let custom_label_mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        remote_label_id.clone(),
    )
    .await
    .expect("failed to create mailbox");
    let custom_label_local_label = custom_label_mailbox.get_local_label(&tether).await;
    let local_conversation_id = local_conversation.id();

    // At least one message is required for this to work.
    let message_id = MessageId::from("my-msg");
    let addr_id = ApiAddress::test_address().id;
    let local_addr_id = Address::remote_id_counterpart(addr_id.clone(), &tether)
        .await
        .unwrap()
        .unwrap();
    let mut message = Message {
        remote_id: Some(message_id.clone()),
        remote_conversation_id: Some(ConversationId::from(remote_conversation_id)),
        label_ids: vec![LabelId::inbox()],
        local_address_id: local_addr_id,
        remote_address_id: addr_id,
        ..Message::test_default()
    };

    tether.tx(async |tx| message.save(tx).await).await.unwrap();

    // Apply label action
    Conversation::action_apply_label(
        user_ctx.action_queue(),
        custom_label_local_label.id(),
        vec![local_conversation_id],
    )
    .await
    .unwrap();

    user_ctx.execute_single_action().await.unwrap();

    // Verify that inbox mailbox contains conversation.
    assert!(
        local_conversation
            .has_label(inbox_local_label.id(), &tether)
            .await
            .expect("Error while checking label presence"),
        "Conversation with ID '{}' should be present in '{}' label, but it isn't.",
        local_conversation_id,
        inbox_local_label.name
    );
    // Verify that custom label mailbox contains conversation.
    assert!(
        local_conversation
            .has_label(custom_label_local_label.id(), &tether)
            .await
            .expect("Error while checking label presence"),
        "Conversation with ID '{}' should be present in '{}' label, but it isn't.",
        local_conversation.id(),
        remote_label_name
    );

    // Apply unlabel action
    Conversation::action_remove_label(
        user_ctx.action_queue(),
        custom_label_local_label.id(),
        vec![local_conversation.id()],
    )
    .await
    .unwrap();

    user_ctx.execute_single_action().await.unwrap();

    // Verify that inbox mailbox contains conversation.
    assert!(
        local_conversation
            .has_label(inbox_local_label.id(), &tether)
            .await
            .expect("Error while checking label presence"),
        "Conversation with ID '{}' should be present in '{}' label, but it isn't.",
        local_conversation.id(),
        inbox_local_label.name
    );
    // Verify that custom label mailbox does NOT contain conversation.
    assert!(
        !local_conversation
            .has_label(custom_label_local_label.id(), &tether)
            .await
            .expect("Error while checking label presence"),
        "Conversation with ID '{}' should NOT be present in '{}' label, but it is.",
        local_conversation.id(),
        remote_label_name
    );
}

/// Validates that it is possible to "star" conversations (i.e. apply Star label) located in Inbox folder.
#[tokio::test]
async fn test_labeling_conversation_with_starred_label() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let remote_conversation_id = "first";
    let inbox_mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();
    let inbox_remote_label = ApiLabel::get_api_label_with_given_id(LabelId::inbox());
    let inbox_local_label = Label::load(inbox_mailbox.label_id(), &tether)
        .await
        .unwrap()
        .unwrap();
    let starred_mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::starred(),
    )
    .await
    .expect("failed to create mailbox");
    let starred_local_label = Label::load(starred_mailbox.label_id(), &tether)
        .await
        .unwrap()
        .unwrap();
    let remote_conversation =
        ApiConversation::test_conversation(remote_conversation_id, vec![inbox_remote_label]);
    let conversations = vec![remote_conversation.clone()];
    let params = test_init_params(hash_map! {}, conversations.clone());

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_label_conversation(
        &LabelId::starred(),
        vec![remote_conversation.id.clone()],
        None,
        vec![],
    )
    .await;
    ctx.mock_unlabel_conversation(
        &LabelId::starred(),
        vec![remote_conversation.id.clone()],
        vec![],
    )
    .await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    inbox_mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let local_conversation = Conversation::load(1.into(), &tether)
        .await
        .unwrap()
        .unwrap();

    // At least one message is required for this to work.
    let message_id = MessageId::from("my-msg");
    let addr_id = ApiAddress::test_address().id;
    let local_addr_id = Address::remote_id_counterpart(addr_id.clone(), &tether)
        .await
        .unwrap()
        .unwrap();
    let mut message = Message {
        remote_id: Some(message_id.clone()),
        remote_conversation_id: Some(ConversationId::from(remote_conversation_id)),
        label_ids: vec![LabelId::inbox()],
        local_address_id: local_addr_id,
        remote_address_id: addr_id,
        ..Message::test_default()
    };

    tether.tx(async |tx| message.save(tx).await).await.unwrap();

    // Apply label action
    Conversation::action_apply_label(
        user_ctx.action_queue(),
        starred_local_label.id(),
        vec![local_conversation.id()],
    )
    .await
    .unwrap();

    user_ctx.execute_single_action().await.unwrap();
    // Verify that inbox mailbox contains conversation.
    assert!(
        local_conversation
            .has_label(inbox_local_label.id(), &tether)
            .await
            .expect("Error while checking label presence"),
        "Conversation with ID '{}' should be present in '{}' label, but it isn't.",
        local_conversation.id(),
        inbox_local_label.clone().name
    );
    // Verify that starred mailbox contains conversation.
    assert!(
        local_conversation
            .has_label(starred_local_label.id(), &tether)
            .await
            .expect("Error while checking label presence"),
        "Conversation with ID '{}' should be present in '{}' label, but it isn't.",
        local_conversation.id(),
        starred_local_label.clone().name
    );

    // Apply unlabel action
    Conversation::action_remove_label(
        user_ctx.action_queue(),
        starred_local_label.clone().id(),
        vec![local_conversation.id()],
    )
    .await
    .unwrap();

    user_ctx.execute_single_action().await.unwrap();

    // Verify that conversation contains label.
    assert!(
        local_conversation
            .has_label(inbox_local_label.id(), &tether)
            .await
            .expect("Error while checking label presence"),
        "Conversation with ID '{}' should be present in '{}' label, but it isn't.",
        local_conversation.id(),
        inbox_local_label.clone().name
    );
    assert!(
        !local_conversation
            .has_label(starred_local_label.id(), &tether)
            .await
            .expect("Error while checking label presence"),
        "Conversation with ID '{}' should NOT be present in '{}' label, but it is.",
        local_conversation.id(),
        starred_local_label.clone().name
    );
}

/// Validates that it is NOT possible to apply a label which is a
/// "folder" (since Folder is a type of label) to conversations.
//TODO(ET-3337): Should use local check not remote check.
#[tokio::test]
#[ignore]
async fn test_labeling_fails_when_labelling_folders() {
    // General setup
    let ctx = MailTestContext::new().await;

    // Set up test data
    let remote_conversation_id = "first";
    let inbox_remote_label = ApiLabel::get_api_label_with_given_id(LabelId::inbox());
    let remote_conversation =
        ApiConversation::test_conversation(remote_conversation_id, vec![inbox_remote_label]);
    let conversations = vec![remote_conversation.clone()];
    let params = test_init_params(hash_map! {}, conversations.clone());

    ctx.setup_user(params).await;
    ctx.mock_get_conversations(conversations, 1).await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let inbox_mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();

    // Sync the mailbox
    inbox_mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            10,
        )
        .await
        .unwrap();

    let label = Label::load(inbox_mailbox.label_id(), &tether)
        .await
        .unwrap()
        .unwrap();

    // Get the local conversation id
    let local_conversation = Conversation::load(1.into(), &tether)
        .await
        .unwrap()
        .unwrap();

    // Label conversation with folder, should fail.
    Conversation::apply_label_to_multiple_remote(
        label.remote_id.unwrap(),
        vec![local_conversation.remote_id.unwrap()],
        None,
        ctx.mail_user_context().await.session(),
    )
    .await
    .unwrap_err();
}

fn test_init_params(
    labels: HashMap<ApiLabelType, Vec<ApiLabel>>,
    conversations: Vec<ApiConversation>,
) -> TestParams {
    let conversation_count = vec![ApiConversationCount {
        label_id: LabelId::inbox().clone(),
        total: conversations.len() as u64,
        unread: 0,
    }];
    let message_count = vec![ApiMessageCount {
        label_id: LabelId::inbox().clone(),
        total: 1,
        unread: 0,
    }];
    TestParams {
        labels,
        addresses: vec![ApiAddress::test_address()],
        conversations,
        conversation_count,
        message_count,
        ..Default::default()
    }
}
