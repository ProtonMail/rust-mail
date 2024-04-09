use proton_api_mail::domain::{
    Address, AddressId, AddressStatus, AddressType, Conversation, ConversationCount,
    ConversationId, ConversationLabels, Label, LabelId, LabelType, MessageCount,
};
use proton_api_mail::exports::crypto::domain::AddressKeys;
use std::collections::HashMap;

mod common;
#[test]
fn test_init_after_login() {
    let ctx = common::TestContext::new();
    let user_ctx = ctx.user_context();
    let mut labels = HashMap::new();
    labels.insert(
        LabelType::Label,
        vec![Label {
            id: LabelId::from("mylabel"),
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
    );

    ctx.async_runtime().block_on(async {
        let init_params = common::init::Params {
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
                id: ConversationId::from("myconv"),
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
        };

        common::init::setup_user(&ctx, init_params).await;
        let cb = common::init::NullCallback {};
        user_ctx
            .initialize_async(LabelId::inbox().clone(), &cb)
            .await
            .expect("failed to initialize");
    });
}
