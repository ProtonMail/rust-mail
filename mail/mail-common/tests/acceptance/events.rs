use proton_core_api::services::proton::prelude::{Address as ApiAddress, Label as ApiLabel};
use proton_core_api::services::proton::{
    Action, AddressFlags, AddressId, AddressStatus, AddressType, EventId, LabelId,
    LabelType as ApiLabelType,
};
use proton_core_common::datatypes::LabelType;
use proton_core_common::models::{Address, Label, ModelIdExtension};
use proton_crypto_account::keys::AddressKeys;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::{
    Conversation as ApiConversation, ConversationEvent, ConversationLabel as ApiConversationLabel,
    MessageMetadata as ApiMessageMetadata,
};
use proton_mail_api::services::proton::response_data::{MailEvent, MessageEvent, MessageFlags};
use proton_mail_common::models::{LabelWithCounters, Message};
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use wiremock::matchers::query_param;

#[tokio::test]
async fn event_fetches_missing_dependencies() {
    let ctx = MailTestContext::new().await;
    let params = Params::default_basic();

    let missing_label_1 = ApiLabel {
        id: LabelId::from("MyLabelId1"),
        parent_id: None,
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: ApiLabelType::Label,
        name: "Missing Label 1".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    };

    let missing_label_2 = ApiLabel {
        id: LabelId::from("MyLabelId2"),
        parent_id: None,
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: ApiLabelType::Folder,
        name: "Missing Label 2".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    };

    let missing_address = ApiAddress {
        id: AddressId::from("MyAddressMissing"),
        address_type: AddressType::Original,
        catch_all: false,
        display_name: "".to_string(),
        domain_id: None,
        email: "".to_string(),
        keys: AddressKeys::new(vec![]),
        order: 0,
        proton_mx: false,
        receive: false,
        send: false,
        signature: "".to_string(),
        signed_key_list: Default::default(),
        status: AddressStatus::Disabled,
        flags: AddressFlags::default(),
    };

    let new_conversation = ApiConversation {
        id: ConversationId::from("MyMissingConvId"),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snoozed_reminder: false,
        expiration_time: 0,
        labels: vec![ApiConversationLabel {
            id: missing_label_1.id.clone(),
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_messages: 0,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        }],
        num_attachments: 0,
        num_messages: 0,
        num_unread: 0,
        order: 0,
        recipients: vec![],
        senders: vec![],
        size: 0,
        subject: "".to_string(),
        context_time: None,
    };

    let new_message = ApiMessageMetadata {
        id: MessageId::from("MyMissingMessageId"),
        conversation_id: new_conversation.id.clone(),
        address_id: missing_address.id.clone(),
        attachments_metadata: vec![],
        bcc_list: vec![],
        cc_list: vec![],
        expiration_time: 0,
        external_id: None,
        flags: MessageFlags::empty(),
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![missing_label_1.id.clone(), missing_label_2.id.clone()],
        num_attachments: 0,
        order: 0,
        sender: Default::default(),
        size: 0,
        snooze_time: 0,
        subject: "".to_string(),
        time: 0,
        to_list: vec![],
        unread: false,
    };

    ctx.core_test_context
        .mock_get_labels_by_ids(vec![missing_label_1.clone(), missing_label_2.clone()])
        .await;
    ctx.core_test_context
        .mock_get_address(missing_address.clone())
        .await;
    ctx.setup_user(params.clone()).await;

    let user_context = ctx.mail_user_context().await;

    let event = MailEvent {
        event_id: EventId::from("MyEventId"),
        labels: None,
        conversation_counts: None,
        conversations: Some(vec![ConversationEvent {
            id: new_conversation.id.clone(),
            action: Action::Create,
            conversation: Some(new_conversation.clone()),
        }]),
        incoming_defaults: None,
        mail_settings: None,
        message_counts: None,
        messages: Some(vec![MessageEvent {
            id: new_message.id.clone(),
            action: Action::Create,
            message: Some(new_message.clone()),
        }]),
        refresh: 0,
        has_more: false,
    };

    user_context.apply_event(event).await.unwrap();
    let tether = user_context.user_stash().connection().await.unwrap();

    // Address, labels and label counters should have been created.
    assert!(
        Address::remote_id_counterpart(missing_address.id, &tether)
            .await
            .unwrap()
            .is_some()
    );
    let local_label_id1 = Label::remote_id_counterpart(missing_label_1.id, &tether)
        .await
        .unwrap()
        .unwrap();
    let local_label_id2 = Label::remote_id_counterpart(missing_label_2.id, &tether)
        .await
        .unwrap()
        .unwrap();

    let labels_and_counters =
        LabelWithCounters::from_ids(&tether, [local_label_id1, local_label_id2])
            .await
            .unwrap();
    assert_eq!(labels_and_counters.len(), 2);
    assert!(
        labels_and_counters
            .iter()
            .any(|l| l.local_id.unwrap() == local_label_id1)
    );
    assert!(
        labels_and_counters
            .iter()
            .any(|l| l.local_id.unwrap() == local_label_id2)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_session_deletion_cleans_mail_caches() {
    // This test verifies that when a session is remotely terminated (e.g., "log out from all devices"),
    // the mail layer properly cleans up mail-specific cache directories.
    use proton_core_common::db::account::CoreSession;
    use proton_core_common::models::ModelExtension;
    use std::time::Duration;

    let ctx = MailTestContext::new().await;
    let params = Params::default_basic();
    ctx.setup_user(params).await;

    let mail_user_ctx = ctx.mail_user_context().await;
    let user_id = mail_user_ctx.user_id().clone();
    let session_id = mail_user_ctx.session_id().clone();

    // Get the mail cache path for this user
    let mail_cache_path = ctx.mail_context.mail_cache_path_for(&user_id);

    // Create a test file in the mail cache to verify cleanup
    tokio::fs::create_dir_all(&mail_cache_path)
        .await
        .expect("Failed to create mail cache directory");
    let test_file = mail_cache_path.join("test_cache.dat");
    tokio::fs::write(&test_file, b"test data")
        .await
        .expect("Failed to write test cache file");

    assert!(test_file.exists(), "Test cache file should exist");

    // Delete the session from the database (simulating remote logout)
    ctx.context
        .account_stash()
        .connection()
        .await
        .unwrap()
        .tx(async |tx| {
            CoreSession::delete_by_id(session_id.clone(), tx).await?;
            Ok::<_, stash::stash::StashError>(())
        })
        .await
        .unwrap();

    // Give the SessionObserver time to detect the change and trigger cleanup
    // Need more time for the observer to pick up the change, run the hook, and complete cleanup
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert!(
        !test_file.exists(),
        "Mail cache file should be cleaned up after session deletion"
    );
    assert!(
        !mail_cache_path.exists()
            || mail_cache_path
                .read_dir()
                .map(|mut d| d.next().is_none())
                .unwrap_or(true),
        "Mail cache directory should be empty or removed"
    );
}

#[tokio::test]
async fn event_fetches_missing_nested_dependencies() {
    let ctx = MailTestContext::new().await;
    let mut params = Params::default_basic();
    params.labels = Default::default();
    // Label 1 -> Label 2 -> Label 3
    let missing_label_1 = ApiLabel {
        id: LabelId::from("MyLabelId1"),
        parent_id: None,
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: ApiLabelType::Folder,
        name: "Missing Label 1".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    };
    let missing_label_2 = ApiLabel {
        id: LabelId::from("MyLabelId2"),
        parent_id: Some(LabelId::from("MyLabelId1")),
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: ApiLabelType::Folder,
        name: "Missing Label 2".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    };
    let missing_label_3 = ApiLabel {
        id: LabelId::from("MyLabelId3"),
        parent_id: Some(LabelId::from("MyLabelId2")),
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: ApiLabelType::Folder,
        name: "Missing Label 3".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    };
    let missing_label_4 = ApiLabel {
        id: LabelId::from("MyLabelId4"),
        parent_id: None,
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: ApiLabelType::Label,
        name: "Missing Label 4".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    };

    let missing_address = ApiAddress {
        id: AddressId::from("MyAddressMissing"),
        address_type: AddressType::Original,
        catch_all: false,
        display_name: "".to_string(),
        domain_id: None,
        email: "".to_string(),
        keys: AddressKeys::new(vec![]),
        order: 0,
        proton_mx: false,
        receive: false,
        send: false,
        signature: "".to_string(),
        signed_key_list: Default::default(),
        status: AddressStatus::Disabled,
        flags: AddressFlags::default(),
    };

    let new_conversation = ApiConversation {
        id: ConversationId::from("MyMissingConvId"),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snoozed_reminder: false,
        expiration_time: 0,
        labels: vec![ApiConversationLabel {
            id: missing_label_3.id.clone(),
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_messages: 0,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        }],
        num_attachments: 0,
        num_messages: 0,
        num_unread: 0,
        order: 0,
        recipients: vec![],
        senders: vec![],
        size: 0,
        subject: "".to_string(),
        context_time: None,
    };

    let new_message = ApiMessageMetadata {
        id: MessageId::from("MyMissingMessageId"),
        conversation_id: new_conversation.id.clone(),
        address_id: missing_address.id.clone(),
        attachments_metadata: vec![],
        bcc_list: vec![],
        cc_list: vec![],
        expiration_time: 0,
        external_id: None,
        flags: MessageFlags::empty(),
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![missing_label_3.id.clone(), missing_label_4.id.clone()],
        num_attachments: 0,
        order: 0,
        sender: Default::default(),
        size: 0,
        snooze_time: 0,
        subject: "".to_string(),
        time: 0,
        to_list: vec![],
        unread: false,
    };

    ctx.setup_user(params.clone()).await;
    let user_context = ctx.mail_user_context().await;

    ctx.core_test_context.mock_server().reset().await;
    ctx.core_test_context
        .mock_get_labels_by_ids(vec![missing_label_3.clone(), missing_label_4.clone()])
        .await;
    ctx.core_test_context
        .mock_get_labels_and(
            vec![
                missing_label_1.clone(),
                missing_label_2.clone(),
                missing_label_3.clone(),
            ],
            |mock| mock.and(query_param("Type", (LabelType::Folder as u8).to_string())),
            1..,
        )
        .await;
    ctx.core_test_context
        .mock_get_address(missing_address.clone())
        .await;

    let event = MailEvent {
        event_id: EventId::from("MyEventId"),
        labels: None,
        conversation_counts: None,
        conversations: Some(vec![ConversationEvent {
            id: new_conversation.id.clone(),
            action: Action::Create,
            conversation: Some(new_conversation.clone()),
        }]),
        incoming_defaults: None,
        mail_settings: None,
        message_counts: None,
        messages: Some(vec![MessageEvent {
            id: new_message.id.clone(),
            action: Action::Create,
            message: Some(new_message.clone()),
        }]),
        refresh: 0,
        has_more: false,
    };

    user_context.apply_event(event).await.unwrap();
    let tether = user_context.user_stash().connection().await.unwrap();

    // Address, labels and label counters should have been created.
    assert!(
        Address::remote_id_counterpart(missing_address.id, &tether)
            .await
            .unwrap()
            .is_some()
    );
    let local_label_id1 = Label::remote_id_counterpart(missing_label_1.id, &tether)
        .await
        .unwrap()
        .unwrap();
    let local_label_id2 = Label::remote_id_counterpart(missing_label_2.id, &tether)
        .await
        .unwrap()
        .unwrap();
    let local_label_id3 = Label::remote_id_counterpart(missing_label_3.id, &tether)
        .await
        .unwrap()
        .unwrap();
    let local_label_id4 = Label::remote_id_counterpart(missing_label_4.id, &tether)
        .await
        .unwrap()
        .unwrap();

    let folder_labels_and_counters = LabelWithCounters::find_by_kind(LabelType::Folder, &tether)
        .await
        .unwrap();
    assert_eq!(folder_labels_and_counters.len(), 3);
    assert!(
        folder_labels_and_counters
            .iter()
            .any(|l| l.local_id.unwrap() == local_label_id1)
    );
    assert!(
        folder_labels_and_counters
            .iter()
            .any(|l| l.local_id.unwrap() == local_label_id2)
    );
    assert!(
        folder_labels_and_counters
            .iter()
            .any(|l| l.local_id.unwrap() == local_label_id3)
    );

    let label_labels_and_counters = LabelWithCounters::find_by_kind(LabelType::Label, &tether)
        .await
        .unwrap();
    assert_eq!(label_labels_and_counters.len(), 1);
    assert!(
        label_labels_and_counters
            .iter()
            .any(|l| l.local_id.unwrap() == local_label_id4)
    );
}

#[tokio::test]
async fn events_skips_unresolved_labels() {
    let ctx = MailTestContext::new().await;
    let params = Params::default_basic();

    let missing_label_1 = ApiLabel {
        id: LabelId::from("MyLabelId1"),
        parent_id: None,
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: ApiLabelType::Label,
        name: "Missing Label 1".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    };

    let missing_label_2 = ApiLabel {
        id: LabelId::from("MyLabelId2"),
        parent_id: Some(LabelId::from("MyLabelId1")),
        color: "".to_string(),
        display: false,
        expanded: false,
        label_type: ApiLabelType::Folder,
        name: "Missing Label 2".to_string(),
        notify: false,
        order: 0,
        path: None,
        sticky: false,
    };

    let missing_address = ApiAddress {
        id: AddressId::from("MyAddressMissing"),
        address_type: AddressType::Original,
        catch_all: false,
        display_name: "".to_string(),
        domain_id: None,
        email: "".to_string(),
        keys: AddressKeys::new(vec![]),
        order: 0,
        proton_mx: false,
        receive: false,
        send: false,
        signature: "".to_string(),
        signed_key_list: Default::default(),
        status: AddressStatus::Disabled,
        flags: AddressFlags::default(),
    };

    let new_conversation = ApiConversation {
        id: ConversationId::from("MyMissingConvId"),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snoozed_reminder: false,
        expiration_time: 0,
        labels: vec![ApiConversationLabel {
            id: missing_label_1.id.clone(),
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_messages: 0,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        }],
        num_attachments: 0,
        num_messages: 0,
        num_unread: 0,
        order: 0,
        recipients: vec![],
        senders: vec![],
        size: 0,
        subject: "".to_string(),
        context_time: None,
    };

    let new_message = ApiMessageMetadata {
        id: MessageId::from("MyMissingMessageId"),
        conversation_id: new_conversation.id.clone(),
        address_id: missing_address.id.clone(),
        attachments_metadata: vec![],
        bcc_list: vec![],
        cc_list: vec![],
        expiration_time: 0,
        external_id: None,
        flags: MessageFlags::empty(),
        is_forwarded: false,
        is_replied: false,
        is_replied_all: false,
        label_ids: vec![missing_label_1.id.clone(), missing_label_2.id.clone()],
        num_attachments: 0,
        order: 0,
        sender: Default::default(),
        size: 0,
        snooze_time: 0,
        subject: "".to_string(),
        time: 0,
        to_list: vec![],
        unread: false,
    };

    let msg_id = new_message.id.clone();

    ctx.core_test_context
        .mock_get_labels_by_ids(vec![missing_label_1.clone()])
        .await;
    ctx.core_test_context
        .mock_get_address(missing_address.clone())
        .await;
    ctx.setup_user(params.clone()).await;

    let user_context = ctx.mail_user_context().await;

    let event = MailEvent {
        event_id: EventId::from("MyEventId"),
        labels: None,
        conversation_counts: None,
        conversations: Some(vec![ConversationEvent {
            id: new_conversation.id.clone(),
            action: Action::Create,
            conversation: Some(new_conversation.clone()),
        }]),
        incoming_defaults: None,
        mail_settings: None,
        message_counts: None,
        messages: Some(vec![MessageEvent {
            id: new_message.id.clone(),
            action: Action::Create,
            message: Some(new_message.clone()),
        }]),
        refresh: 0,
        has_more: false,
    };

    user_context.apply_event(event).await.unwrap();
    let tether = user_context.user_stash().connection().await.unwrap();

    // Address, labels and label counters should have been created.
    assert!(
        Address::remote_id_counterpart(missing_address.id, &tether)
            .await
            .unwrap()
            .is_some()
    );

    assert!(
        Label::remote_id_counterpart(missing_label_2.id.clone(), &tether)
            .await
            .unwrap()
            .is_none()
    );

    let message = Message::find_by_remote_id(msg_id, &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(!message.label_ids.contains(&missing_label_2.id));
    assert!(message.label_ids.contains(&missing_label_1.id));
}

#[cfg(feature = "events-v6")]
mod v6 {
    use proton_core_common::models::ModelExtension;
    use proton_mail_api::services::proton::prelude::{
        ConversationCount, MailConversationEventV6, MailEventV6, MailLabelEventV6,
        MailMessageEventV6, MessageCount,
    };
    use proton_mail_common::{
        datatypes::SystemLabelId,
        models::{Conversation, ConversationCounter, MessageCounter},
    };

    use super::*;

    #[tokio::test]
    async fn events_fetches_relevant_counters() {
        let ctx = MailTestContext::new().await;
        let params = Params::default_basic();

        let conv_id = ConversationId::from("ConvId");
        let msg_id = MessageId::from("MessageId");
        let label_id = LabelId::from("Folder");

        let new_label = ApiLabel {
            id: label_id.clone(),
            parent_id: None,
            color: "".to_string(),
            display: false,
            expanded: false,
            label_type: ApiLabelType::Folder,
            name: "Folder".to_string(),
            notify: false,
            order: 0,
            path: None,
            sticky: false,
        };

        let new_conversation = ApiConversation {
            id: conv_id.clone(),
            attachment_info: Default::default(),
            attachments_metadata: vec![],
            display_snoozed_reminder: false,
            expiration_time: 0,
            labels: vec![ApiConversationLabel {
                id: LabelId::starred(),
                context_expiration_time: 0,
                context_num_attachments: 0,
                context_num_messages: 0,
                context_num_unread: 0,
                context_size: 0,
                context_snooze_time: 0,
                context_time: 0,
            }],
            num_attachments: 0,
            num_messages: 0,
            num_unread: 0,
            order: 0,
            recipients: vec![],
            senders: vec![],
            size: 0,
            subject: "".to_string(),
            context_time: None,
        };

        let new_message = ApiMessageMetadata {
            id: msg_id.clone(),
            conversation_id: new_conversation.id.clone(),
            address_id: params.addresses[0].id.clone(),
            attachments_metadata: vec![],
            bcc_list: vec![],
            cc_list: vec![],
            expiration_time: 0,
            external_id: None,
            flags: MessageFlags::empty(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: vec![LabelId::starred()],
            num_attachments: 0,
            order: 0,
            sender: Default::default(),
            size: 0,
            snooze_time: 0,
            subject: "".to_string(),
            time: 0,
            to_list: vec![],
            unread: false,
        };

        let starred_msg_count = MessageCount {
            label_id: LabelId::starred(),
            total: 100,
            unread: 200,
        };
        let label_msg_count = MessageCount {
            label_id: label_id.clone(),
            total: 400,
            unread: 300,
        };

        let starred_conv_count = ConversationCount {
            label_id: LabelId::starred(),
            total: 1000,
            unread: 2000,
        };
        let label_conv_count = ConversationCount {
            label_id: label_id.clone(),
            total: 4000,
            unread: 3000,
        };

        ctx.setup_user(params.clone()).await;

        let user_context = ctx.mail_user_context().await;

        ctx.mock_server().reset().await;

        ctx.mock_get_labels_by_ids(vec![new_label]).await;
        ctx.mock_get_message_metadata_page(vec![new_message.clone()], None, None, 50, 1, 1)
            .await;
        ctx.mock_get_conversations_page(vec![new_conversation.clone()], None, None, 50, 1, 1)
            .await;

        ctx.mock_get_messages_count(
            Some(vec![starred_msg_count.clone(), label_msg_count.clone()]),
            1,
        )
        .await;
        ctx.mock_get_conversations_count(
            Some(vec![starred_conv_count.clone(), label_conv_count.clone()]),
            1,
        )
        .await;

        let event = MailEventV6 {
            event_id: EventId::from("MyEventId"),
            labels: Some(vec![MailLabelEventV6 {
                id: label_id.clone(),
                action: Action::Create,
            }]),
            conversations: Some(vec![MailConversationEventV6 {
                id: new_conversation.id.clone(),
                action: Action::Create,
            }]),
            incoming_defaults: None,
            mail_settings: None,
            messages: Some(vec![MailMessageEventV6 {
                id: new_message.id.clone(),
                action: Action::Create,
            }]),
            refresh: false,
            has_more: false,
        };

        user_context.apply_mail_event_v6(event).await.unwrap();
        let tether = user_context.user_stash().connection().await.unwrap();

        let local_label_id = Label::remote_id_counterpart(label_id.clone(), &tether)
            .await
            .unwrap()
            .unwrap();
        let local_starred_id = Label::remote_id_counterpart(LabelId::starred(), &tether)
            .await
            .unwrap()
            .unwrap();
        assert!(
            Message::remote_id_counterpart(msg_id.clone(), &tether)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            Conversation::remote_id_counterpart(conv_id.clone(), &tether)
                .await
                .unwrap()
                .is_some()
        );

        let msg_counter_starred = MessageCounter::find_by_id(local_starred_id, &tether)
            .await
            .unwrap()
            .unwrap();
        let msg_counter_label = MessageCounter::find_by_id(local_label_id, &tether)
            .await
            .unwrap()
            .unwrap();

        let conv_counter_starred = ConversationCounter::find_by_id(local_starred_id, &tether)
            .await
            .unwrap()
            .unwrap();
        let conv_counter_label = ConversationCounter::find_by_id(local_label_id, &tether)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(msg_counter_label.unread, label_msg_count.unread);
        assert_eq!(msg_counter_label.total, label_msg_count.total);
        assert_eq!(msg_counter_starred.unread, starred_msg_count.unread);
        assert_eq!(msg_counter_starred.total, starred_msg_count.total);

        assert_eq!(conv_counter_label.unread, label_conv_count.unread);
        assert_eq!(conv_counter_label.total, label_conv_count.total);
        assert_eq!(conv_counter_starred.unread, starred_conv_count.unread);
        assert_eq!(conv_counter_starred.total, starred_conv_count.total);
    }
}
