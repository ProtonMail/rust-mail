use proton_core_api::services::proton::prelude::{Address as ApiAddress, Label as ApiLabel};
use proton_core_api::services::proton::{
    Action, AddressFlags, AddressId, AddressStatus, AddressType, EventId, LabelId, LabelType,
};
use proton_core_common::models::{Address, Label, ModelIdExtension};
use proton_crypto_account::keys::AddressKeys;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::{
    Conversation as ApiConversation, ConversationEvent, ConversationLabel as ApiConversationLabel,
    MessageMetadata as ApiMessageMetadata,
};
use proton_mail_api::services::proton::response_data::{MailEvent, MessageEvent, MessageFlags};
use proton_mail_common::models::LabelWithCounters;
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};

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
        label_type: LabelType::Label,
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
        label_type: LabelType::Folder,
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
    ctx.catch_all().await;

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

    user_context.apply_event(event.into()).await.unwrap();
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
