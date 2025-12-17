use crate::acceptance::drafts_common::draft_test_params;
use proton_core_api::services::proton::{Action, AddressEvent, CoreEvent, EventId, UserId};
use proton_core_common::test_utils::test_context::UserContextTestExtension;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::{
    ConversationId, MailEvent, MessageEvent, MessageMetadata,
};
use proton_mail_api::services::proton::response_data::MessageFlags;
use proton_mail_common::models::Message;
use proton_mail_common::test_utils::message_body::{TEST_USER_ID, message_body_test_user_secret};
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use stash::orm::Model;
use stash::params;

#[tokio::test]
async fn address_delete_event() {
    // While this is implemented in core, we test in mail to make sure that things
    // that depend on addresses are deleted properly and the db operation does not fail.
    let ctx = MailTestContext::with_user_secret_and_user_id(
        message_body_test_user_secret(),
        UserId::from(TEST_USER_ID),
    )
    .await;
    let params = draft_test_params();
    let address_ids = params
        .addresses
        .iter()
        .map(|a| a.id.clone())
        .collect::<Vec<_>>();
    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    // crate a message with this address.
    let msgs = address_ids
        .iter()
        .enumerate()
        .map(|(idx, id)| MessageMetadata {
            id: MessageId::new(format!("msg-{idx}")),
            conversation_id: ConversationId::new(format!("conv-{idx}")),
            address_id: id.clone(),
            attachments_metadata: vec![],
            bcc_list: vec![],
            cc_list: vec![],
            expiration_time: 0,
            external_id: None,
            flags: MessageFlags::empty(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: vec![],
            num_attachments: 0,
            order: 0,
            sender: Default::default(),
            size: 0,
            snooze_time: 0,
            subject: "".to_string(),
            time: 0,
            to_list: vec![],
            unread: false,
        })
        .collect::<Vec<_>>();

    user_ctx
        .apply_event(MailEvent {
            event_id: EventId::from("event"),
            labels: None,
            conversation_counts: None,
            conversations: None,
            incoming_defaults: None,
            mail_settings: None,
            message_counts: None,
            messages: Some(
                msgs.into_iter()
                    .map(|m| MessageEvent {
                        id: m.id.clone(),
                        action: Action::Create,
                        message: Some(m),
                    })
                    .collect(),
            ),
            refresh: 0,
            has_more: false,
        })
        .await
        .unwrap();

    let msg_count = Message::count("", params![], &tether).await.unwrap();
    assert_eq!(msg_count, 2);

    user_ctx
        .user_context()
        .as_arc()
        .apply_event(&CoreEvent {
            event_id: EventId::from("Foo"),
            addresses: Some(
                address_ids
                    .into_iter()
                    .map(|id| AddressEvent {
                        id,
                        action: Action::Delete,
                        address: None,
                    })
                    .collect(),
            ),
            labels: None,
            product_used_space: None,
            used_space: None,
            user: None,
            user_settings: None,
            contacts: None,
            refresh: 0,
            has_more: false,
        })
        .await
        .unwrap();

    let msg_count = Message::count("", params![], &tether).await.unwrap();
    assert_eq!(msg_count, 0);
}
