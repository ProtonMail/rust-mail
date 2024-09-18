mod common;

use crate::common::TestContext;
use common::init::{NullCallback, Params as TestParams};
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    ConversationLabel as ApiConversationLabel, MessageCount as ApiMessageCount, MessageFlags,
    MessageMetadata as ApiMessageMetadata, MessageMetadata,
};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_crypto_account::keys::AddressKeys as ApiAddressKeys;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, Message};
use proton_mail_common::{MailUserContext, Mailbox};
use std::collections::HashMap;

#[tokio::test]
async fn paginate_conversations() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let page_size = 2_usize;
    let conversation_count = 6_usize;
    let (init_params, _) = test_init_params(conversation_count);
    let mut conversations = init_params.conversations.clone();
    conversations.reverse();
    let page_chunks = conversations.chunks(page_size).collect::<Vec<_>>();

    ctx.setup_user(init_params).await;
    // first page sync
    ctx.mock_get_conversations_page(
        page_chunks[0].to_vec(),
        None,
        None,
        page_size.try_into().unwrap(),
        conversation_count.try_into().unwrap(),
        1,
    )
    .await;

    // Second page sync.
    {
        let last_page_0_item = page_chunks[0].last().unwrap();
        ctx.mock_get_conversations_page(
            std::iter::once(last_page_0_item.clone())
                .chain(page_chunks[1].to_vec().into_iter())
                .collect(),
            Some(last_page_0_item.id.clone().into()),
            Some(last_page_0_item.labels[0].context_time),
            page_size as u64 + 1_u64,
            (conversation_count - page_size) as u64,
            1,
        )
        .await;
    }

    // Third page sync.
    {
        let last_page_1_item = page_chunks[1].last().unwrap();
        ctx.mock_get_conversations_page(
            std::iter::once(last_page_1_item.clone())
                .chain(page_chunks[2].to_vec().into_iter())
                .collect(),
            Some(last_page_1_item.id.clone().into()),
            Some(last_page_1_item.labels[0].context_time),
            page_size as u64 + 1_u64,
            (conversation_count - page_size * 2) as u64,
            1,
        )
        .await;
    }

    // Last page sync.
    {
        let last_page_2_item = page_chunks[2].last().unwrap();
        ctx.mock_get_conversations_page(
            vec![last_page_2_item.clone().into()],
            Some(last_page_2_item.id.clone().into()),
            Some(last_page_2_item.labels[0].context_time),
            page_size as u64 + 1_u64,
            1,
            1,
        )
        .await;
    }

    ctx.catch_all().await;
    let cb = NullCallback {};
    user_ctx
        .initialize_async(&cb)
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    let paginator = Conversation::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        page_size.try_into().unwrap(),
        None,
    )
    .await
    .unwrap();

    let page1 = paginator.current_page().await.unwrap();
    compare_conversations(&user_ctx, &page1, &page_chunks[0]).await;

    // page 2
    let page2 = paginator.next_page().await.unwrap();
    compare_conversations(&user_ctx, &page2, &page_chunks[1]).await;

    // page 3
    let page3 = paginator.next_page().await.unwrap();
    compare_conversations(&user_ctx, &page3, &page_chunks[2]).await;

    // page 4, no more values
    let page4 = paginator.next_page().await.unwrap();
    assert!(page4.is_empty());
}

#[tokio::test]
async fn paginate_messages() {
    let ctx = TestContext::new().await;
    let user_ctx = ctx.user_context().await;

    let page_size = 2_usize;
    let conversation_count = 6_usize;
    let (init_params, mut messages) = test_init_params(conversation_count);
    messages.reverse();
    let page_chunks = messages.chunks(page_size).collect::<Vec<_>>();

    ctx.setup_user(init_params).await;
    // first page sync
    ctx.mock_get_message_metadata_page(
        page_chunks[0].to_vec(),
        None,
        None,
        page_size.try_into().unwrap(),
        conversation_count.try_into().unwrap(),
        1,
    )
    .await;

    // Second page sync.
    {
        let last_page_0_item = page_chunks[0].last().unwrap();
        ctx.mock_get_message_metadata_page(
            std::iter::once(last_page_0_item.clone())
                .chain(page_chunks[1].to_vec().into_iter())
                .collect(),
            Some(last_page_0_item.id.clone().into()),
            None,
            page_size as u64 + 1_u64,
            (conversation_count - page_size) as u64,
            1,
        )
        .await;
    }

    // Third page sync.
    {
        let last_page_1_item = page_chunks[1].last().unwrap();
        ctx.mock_get_message_metadata_page(
            std::iter::once(last_page_1_item.clone())
                .chain(page_chunks[2].to_vec().into_iter())
                .collect(),
            Some(last_page_1_item.id.clone().into()),
            None,
            page_size as u64 + 1_u64,
            (conversation_count - page_size * 2) as u64,
            1,
        )
        .await;
    }

    // Last page sync.
    {
        let last_page_2_item = page_chunks[2].last().unwrap();
        ctx.mock_get_message_metadata_page(
            vec![last_page_2_item.clone().into()],
            Some(last_page_2_item.id.clone().into()),
            None,
            page_size as u64 + 1_u64,
            1,
            1,
        )
        .await;
    }

    ctx.catch_all().await;
    let cb = NullCallback {};
    user_ctx
        .initialize_async(&cb)
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        page_size.try_into().unwrap(),
        None,
    )
    .await
    .unwrap();

    let page1 = paginator.current_page().await.unwrap();
    compare_messages(&user_ctx, &page1, &page_chunks[0]).await;

    // page 2
    let page2 = paginator.next_page().await.unwrap();
    compare_messages(&user_ctx, &page2, &page_chunks[1]).await;

    // page 3
    let page3 = paginator.next_page().await.unwrap();
    compare_messages(&user_ctx, &page3, &page_chunks[2]).await;

    // page 4, no more values
    let page4 = paginator.next_page().await.unwrap();
    assert!(page4.is_empty());
}

async fn compare_conversations(
    user_ctx: &MailUserContext,
    page: &[Conversation],
    api: &[ApiConversation],
) {
    for (local_conv, api_conv) in std::iter::zip(page, api) {
        let api_local_conv = Conversation::find_by_id::<RemoteId, _>(
            api_conv.id.clone().into(),
            user_ctx.user_stash(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(*local_conv, api_local_conv);
    }
}
async fn compare_messages(
    user_ctx: &MailUserContext,
    page: &[Message],
    api: &[ApiMessageMetadata],
) {
    for (local_conv, api_conv) in std::iter::zip(page, api) {
        let api_local_conv =
            Message::find_by_id::<RemoteId, _>(api_conv.id.clone().into(), user_ctx.user_stash())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(*local_conv, api_local_conv);
    }
}

fn test_init_params(count: usize) -> (TestParams, Vec<MessageMetadata>) {
    let new_conversation_labels = |index| {
        vec![ApiConversationLabel {
            id: LabelId::inbox().into(),
            context_num_unread: 0,
            context_num_messages: 1,
            context_time: item_time(index),
            context_size: 12,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
        }]
    };
    let params = TestParams {
        last_event_id: None,
        user_info: None,
        user_settings: None,
        mail_settings: None,
        labels: HashMap::default(),
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
        conversations: (0..count)
            .into_iter()
            .map(|i| ApiConversation {
                id: conversation_id(i),
                order: (i + 1).try_into().unwrap(),
                subject: "Hello".to_owned(),
                senders: vec![],
                recipients: vec![],
                num_messages: 1,
                num_unread: 0,
                num_attachments: 0,
                expiration_time: 0,
                size: 12,
                labels: new_conversation_labels(i),
                display_snooze_reminder: false,
                attachments_metadata: vec![],
                attachment_info: Default::default(),
            })
            .collect(),
        attachments: vec![],
        conversation_count: vec![ApiConversationCount {
            label_id: LabelId::inbox().clone().into(),
            total: count.try_into().unwrap(),
            unread: 0,
        }],
        message_count: vec![ApiMessageCount {
            label_id: LabelId::inbox().clone().into(),
            total: count.try_into().unwrap(),
            unread: 0,
        }],
    };

    let messages = (0..count)
        .into_iter()
        .map(|i| ApiMessageMetadata {
            id: message_id(i),
            conversation_id: conversation_id(i),
            address_id: ApiRemoteId::from("myaddress"),
            attachments_metadata: vec![],
            bcc_list: vec![],
            cc_list: vec![],
            expiration_time: 0,
            external_id: None,
            flags: MessageFlags::empty(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: vec![LabelId::inbox().into()],
            num_attachments: 0,
            order: (i + 1).try_into().unwrap(),
            reply_tos: vec![],
            sender: Default::default(),
            size: 0,
            snooze_time: 0,
            subject: "Hello".to_string(),
            time: item_time(i),
            to_list: vec![],
            unread: false,
        })
        .collect();

    (params, messages)
}

fn item_time(index: usize) -> u64 {
    (index * 100).try_into().unwrap()
}

fn conversation_id(index: usize) -> ApiRemoteId {
    ApiRemoteId::from(format!("conv-{index}"))
}

fn message_id(index: usize) -> ApiRemoteId {
    ApiRemoteId::from(format!("msg-{index}"))
}
