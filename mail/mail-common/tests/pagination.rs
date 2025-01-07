use proton_api_core::services::proton::common::{AddressId, LabelId, RemoteId as ApiRemoteId};
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use proton_api_mail::services::proton::common::MessageId;
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, ConversationCount as ApiConversationCount,
    ConversationLabel as ApiConversationLabel, MessageCount as ApiMessageCount, MessageFlags,
    MessageMetadata as ApiMessageMetadata, MessageMetadata,
};
use proton_api_mail::services::proton::responses::{GetConversationsResponse, GetMessagesResponse};
use proton_core_common::db::migrations::migrate_core_db;
use proton_core_common::models::ModelIdExtension;
use proton_crypto_account::keys::AddressKeys as ApiAddressKeys;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::db::migrations::migrate_db;
use proton_mail_common::models::PaginatorSearchOptions;
use proton_mail_common::models::{Conversation, Message, PaginatorFilter};
use proton_mail_common::{MailUserContext, Mailbox};
use proton_mail_test_utils::init::{NullCallback, Params as TestParams};
use proton_mail_test_utils::test_context::MailTestContext;
use proton_mail_test_utils::utils::create_address;
use proton_mail_test_utils::{conversation, message};
use stash::orm::Model;
use stash::params;
use std::sync::Arc;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn paginate_conversations() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;

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
                .chain(page_chunks[1].iter().cloned())
                .collect(),
            Some(last_page_0_item.id.clone()),
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
                .chain(page_chunks[2].iter().cloned())
                .collect(),
            Some(last_page_1_item.id.clone()),
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
            vec![last_page_2_item.clone()],
            Some(last_page_2_item.id.clone()),
            Some(last_page_2_item.labels[0].context_time),
            page_size as u64 + 1_u64,
            1,
            1,
        )
        .await;
    }

    ctx.catch_all().await;
    let cb = NullCallback {};
    MailUserContext::initialize_async(Arc::clone(&user_ctx), &cb)
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    let paginator = Conversation::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        page_size.try_into().unwrap(),
        PaginatorFilter::default(),
        true,
    )
    .await
    .unwrap();

    let page1 = paginator.next_page().await.unwrap();
    compare_conversations(&user_ctx, &page1, page_chunks[0]).await;

    // page 2
    let page2 = paginator.next_page().await.unwrap();
    compare_conversations(&user_ctx, &page2, page_chunks[1]).await;

    // page 3
    let page3 = paginator.next_page().await.unwrap();
    compare_conversations(&user_ctx, &page3, page_chunks[2]).await;

    // page 4, no more values
    let page4 = paginator.next_page().await.unwrap();
    assert!(page4.is_empty());
}

#[tokio::test]
async fn paginate_messages() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;

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
                .chain(page_chunks[1].iter().cloned())
                .collect(),
            Some(last_page_0_item.id.clone()),
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
                .chain(page_chunks[2].iter().cloned())
                .collect(),
            Some(last_page_1_item.id.clone()),
            Some(last_page_1_item.time),
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
            vec![last_page_2_item.clone()],
            Some(last_page_2_item.id.clone()),
            Some(last_page_2_item.time),
            page_size as u64 + 1_u64,
            1,
            1,
        )
        .await;
    }

    ctx.catch_all().await;
    let cb = NullCallback {};
    MailUserContext::initialize_async(Arc::clone(&user_ctx), &cb)
        .await
        .expect("failed to initialize");

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        page_size.try_into().unwrap(),
        PaginatorFilter::default(),
        PaginatorSearchOptions::default(),
        true,
    )
    .await
    .unwrap();

    let page1 = paginator.next_page().await.unwrap();
    compare_messages(&user_ctx, &page1, page_chunks[0]).await;

    // page 2
    let page2 = paginator.next_page().await.unwrap();
    compare_messages(&user_ctx, &page2, page_chunks[1]).await;

    // page 3
    let page3 = paginator.next_page().await.unwrap();
    compare_messages(&user_ctx, &page3, page_chunks[2]).await;

    // page 4, no more values
    let page4 = paginator.next_page().await.unwrap();
    assert!(page4.is_empty());
}

async fn compare_conversations(
    user_ctx: &MailUserContext,
    page: &[Conversation],
    api: &[ApiConversation],
) {
    let tether = user_ctx.user_stash().connection();
    for (local_conv, api_conv) in std::iter::zip(page, api) {
        let api_local_conv = Conversation::find_by_remote_id(api_conv.id.clone(), &tether)
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
    let tether = user_ctx.user_stash().connection();
    for (local_conv, api_conv) in std::iter::zip(page, api) {
        let api_local_conv = Message::find_by_remote_id(api_conv.id.clone(), &tether)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(*local_conv, api_local_conv);
    }
}

fn test_init_params(count: usize) -> (TestParams, Vec<MessageMetadata>) {
    let new_conversation_labels = |index| {
        vec![ApiConversationLabel {
            id: LabelId::inbox(),
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
        conversations: (0..count)
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
                ..Default::default()
            })
            .collect(),
        attachments: vec![],
        conversation_count: vec![ApiConversationCount {
            label_id: LabelId::inbox().clone(),
            total: count.try_into().unwrap(),
            unread: 0,
        }],
        message_count: vec![ApiMessageCount {
            label_id: LabelId::inbox().clone(),
            total: count.try_into().unwrap(),
            unread: 0,
        }],
        ..Default::default()
    };

    let messages = (0..count)
        .map(|i| ApiMessageMetadata {
            id: message_id(i),
            conversation_id: conversation_id(i),
            address_id: AddressId::from("myaddress"),
            attachments_metadata: vec![],
            bcc_list: vec![],
            cc_list: vec![],
            expiration_time: 0,
            external_id: None,
            flags: MessageFlags::empty(),
            is_forwarded: false,
            is_replied: false,
            is_replied_all: false,
            label_ids: vec![LabelId::inbox()],
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

#[tokio::test]
async fn paginate_conversations_for_label_with_filter() {
    // Create a test context
    let context = MailTestContext::new().await;
    let user_ctx = context.mail_user_context().await;
    let stash = user_ctx.user_stash();
    migrate_core_db(stash).await.unwrap();
    migrate_db(stash).await.unwrap();

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    Mock::given(method("GET"))
        .and(path("/api/mail/v4/conversations"))
        .and(query_param("PageSize", 50.to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                conversations: vec![],
                stale: false,
                total: 1,
            }),
        )
        .expect(2)
        .mount(context.mock_server())
        .await;

    // Create 5 conversations: 3 with unread messages, 2 without
    let conversations = vec![
        conversation!(num_unread: 1, remote_id: Some("conv1".into())),
        conversation!(num_unread: 0, remote_id: Some("conv2".into())),
        conversation!(num_unread: 2, remote_id: Some("conv3".into())),
        conversation!(num_unread: 0, remote_id: Some("conv4".into())),
        conversation!(num_unread: 3, remote_id: Some("conv5".into())),
    ];

    let mut tether = stash.connection();
    let tx = tether.transaction().await.unwrap();
    for mut conv in conversations {
        conv.save(&tx).await.expect("failed to create conversation");
        Conversation::apply_label(mailbox_inbox.label_id(), vec![conv.local_id.unwrap()], &tx)
            .await
            .unwrap();
        tx.execute(
            "
            UPDATE
                conversation_labels
            SET
                context_num_unread = ?
            WHERE
                local_label_id = ?
                AND local_conversation_id = ?
            ",
            params![conv.num_unread, mailbox_inbox.label_id(), conv.id()],
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();

    // Test with unread filter
    let filter = PaginatorFilter { unread: Some(true) };
    let paginator =
        Conversation::paginate_in_label(&user_ctx, mailbox_inbox.label_id(), 50, filter, true)
            .await
            .unwrap();

    let conversations = paginator.next_page().await.unwrap();
    assert_eq!(
        conversations.len(),
        3,
        "Expected 3 conversations with unread messages"
    );
    assert!(
        conversations.iter().all(|c| c.num_unread > 0),
        "All conversations should have unread messages"
    );

    // Test without filter (should return all conversations)
    let filter = PaginatorFilter { unread: None };
    let paginator =
        Conversation::paginate_in_label(&user_ctx, mailbox_inbox.label_id(), 50, filter, true)
            .await
            .unwrap();

    let conversations = paginator.next_page().await.unwrap();
    assert_eq!(conversations.len(), 5, "Expected all 5 conversations");

    // Test with read filter
    let filter = PaginatorFilter {
        unread: Some(false),
    };
    let paginator =
        Conversation::paginate_in_label(&user_ctx, mailbox_inbox.label_id(), 50, filter, true)
            .await
            .unwrap();

    let conversations = paginator.next_page().await.unwrap();
    assert_eq!(
        conversations.len(),
        2,
        "Expected 2 conversations without unread messages"
    );
    assert!(
        conversations.iter().all(|c| c.num_unread == 0),
        "All conversations should have no unread messages"
    );
}

#[tokio::test]
async fn paginate_messages_for_label_with_filter() {
    // Create a test context
    let context = MailTestContext::new().await;
    let user_ctx = context.mail_user_context().await;
    let stash = user_ctx.user_stash();
    migrate_core_db(stash).await.unwrap();
    migrate_db(stash).await.unwrap();

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    Mock::given(method("GET"))
        .and(path("/api/mail/v4/messages"))
        .and(query_param("PageSize", 50.to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                messages: vec![],
                stale: false,
                total: 1,
            }),
        )
        .expect(2)
        .mount(context.mock_server())
        .await;

    // Set up test data
    let mut tether = stash.connection();
    let address = create_address(&mut tether).await;
    let mut conversation = conversation!(remote_id: Some("test_conversation".into()));
    let tx = tether.transaction().await.unwrap();
    conversation.save(&tx).await.unwrap();

    // Create 5 messages: 3 unread, 2 read
    let messages = vec![
        message!(unread: true, remote_id: Some("msg1".into())),
        message!(unread: false, remote_id: Some("msg2".into())),
        message!(unread: true, remote_id: Some("msg3".into())),
        message!(unread: false, remote_id: Some("msg4".into())),
        message!(unread: true, remote_id: Some("msg5".into())),
    ];

    for mut msg in messages {
        msg.local_address_id = address.local_id.unwrap();
        msg.remote_address_id = address.remote_id.clone().unwrap();
        msg.local_conversation_id = conversation.local_id;
        msg.remote_conversation_id = conversation.remote_id.clone();
        msg.save(&tx).await.expect("failed to create message");
        Message::apply_label(mailbox_inbox.label_id(), vec![msg.local_id.unwrap()], &tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();

    // Test with unread filter
    let filter = PaginatorFilter { unread: Some(true) };
    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        50,
        filter,
        PaginatorSearchOptions::default(),
        true,
    )
    .await
    .unwrap();

    let messages = paginator.next_page().await.unwrap();
    assert_eq!(messages.len(), 3, "Expected 3 unread messages");
    assert!(
        messages.iter().all(|m| m.unread),
        "All messages should be unread"
    );

    // Test without filter (should return all messages)
    let filter = PaginatorFilter { unread: None };
    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        50,
        filter,
        PaginatorSearchOptions::default(),
        true,
    )
    .await
    .unwrap();

    let messages = paginator.next_page().await.unwrap();
    assert_eq!(messages.len(), 5, "Expected all 5 messages");

    // Test with read filter
    let filter = PaginatorFilter {
        unread: Some(false),
    };
    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        50,
        filter,
        PaginatorSearchOptions::default(),
        true,
    )
    .await
    .unwrap();

    let messages = paginator.next_page().await.unwrap();
    assert_eq!(messages.len(), 2, "Expected 2 read messages");
    assert!(
        messages.iter().all(|m| !m.unread),
        "All messages should be read"
    );
}

#[tokio::test]
async fn paginate_search() {
    // Create a test context
    let context = MailTestContext::new().await;
    let user_ctx = context.mail_user_context().await;
    let stash = user_ctx.user_stash();
    migrate_core_db(stash).await.unwrap();
    migrate_db(stash).await.unwrap();

    let mailbox_inbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .expect("failed to create mailbox");

    Mock::given(method("GET"))
        .and(path("/api/mail/v4/messages"))
        .and(query_param("PageSize", 50.to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                messages: vec![],
                stale: false,
                total: 1,
            }),
        )
        .expect(2)
        .mount(context.mock_server())
        .await;

    // Set up test data
    let mut tether = stash.connection();
    let address = create_address(&mut tether).await;
    let mut conversation = conversation!(remote_id: Some("test_conversation".into()));
    let tx = tether.transaction().await.unwrap();
    conversation.save(&tx).await.unwrap();

    // Create 5 messages
    let messages = vec![
        message!(subject: "foo".to_owned(), remote_id: Some("msg1".into())),
        message!(subject: "bar".to_owned(), remote_id: Some("msg2".into())),
        message!(subject: "foo bar".to_owned(), remote_id: Some("msg3".into())),
        message!(subject: "baz".to_owned(), remote_id: Some("msg4".into())),
        message!(subject: "bar baz".to_owned(), remote_id: Some("msg5".into())),
    ];

    for mut msg in messages {
        msg.local_address_id = address.local_id.unwrap();
        msg.remote_address_id = address.remote_id.clone().unwrap();
        msg.local_conversation_id = conversation.local_id;
        msg.remote_conversation_id = conversation.remote_id.clone();
        msg.save(&tx).await.expect("failed to create message");
        Message::apply_label(mailbox_inbox.label_id(), vec![msg.local_id.unwrap()], &tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();

    // Test with search term "foo"
    let options = PaginatorSearchOptions {
        keywords: Some("foo".to_owned()),
    };
    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        50,
        PaginatorFilter::default(),
        options,
        true,
    )
    .await
    .unwrap();

    let messages = paginator.next_page().await.unwrap();
    assert_eq!(messages.len(), 2, "Expected 2 matching messages");

    // Test without filter (should return all messages)
    let options = PaginatorSearchOptions { keywords: None };
    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        50,
        PaginatorFilter::default(),
        options,
        true,
    )
    .await
    .unwrap();

    let messages = paginator.next_page().await.unwrap();
    assert_eq!(messages.len(), 5, "Expected all 5 messages");

    // Test with multiple search terms
    let options = PaginatorSearchOptions {
        keywords: Some("foo bar".to_owned()),
    };
    let paginator = Message::paginate_in_label(
        &user_ctx,
        mailbox_inbox.label_id(),
        50,
        PaginatorFilter::default(),
        options,
        true,
    )
    .await
    .unwrap();

    let messages = paginator.next_page().await.unwrap();
    assert_eq!(messages.len(), 1, "Expected 1 matching message");
}

fn item_time(index: usize) -> u64 {
    (index * 100).try_into().unwrap()
}

fn conversation_id(index: usize) -> ApiRemoteId {
    ApiRemoteId::from(format!("conv-{index}"))
}

fn message_id(index: usize) -> MessageId {
    MessageId::from(format!("msg-{index}"))
}
