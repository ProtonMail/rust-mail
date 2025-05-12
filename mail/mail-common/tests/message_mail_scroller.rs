use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::SystemLabel,
    models::{Address, Label, ModelExtension, ModelIdExtension},
};
use proton_mail_api::services::proton::{
    common::MessageId, prelude::GetMessagesResponse,
    response_data::MessageMetadata as ApiMessageMetadata,
};
use proton_mail_common::{
    datatypes::ReadFilter,
    mail_scroller::MailScroller,
    models::{Conversation, Message, MessageCounters, MessageScrollData},
};
use proton_mail_test_utils::{api_message_meta, utils::create_address};
use proton_mail_test_utils::{conv_id, conversation, label, lbl_id, message, msg_id};
use proton_mail_test_utils::{init::Params as TestParams, test_context::MailTestContext};
use velcro::btree_map;

use stash::stash::StashError;
use stash::{
    orm::Model,
    stash::{Bond, Tether, WatcherHandle},
};
use std::{collections::BTreeMap, vec};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path, query_param_contains},
};

fn test_message(n: usize, order_shift: u64) -> Vec<Message> {
    (0..n)
        .map(|i| {
            let order = i as u64 + order_shift;
            message!(remote_id: msg_id!(order),  display_order: order, time: order)
        })
        .collect()
}

async fn save_single_message(label: &Label, message: &mut Message, bond: &Bond<'_>) {
    message.label_ids = vec![label.remote_id.clone().unwrap()];
    message.save(bond).await.unwrap();
    message.reload(bond).await.unwrap();
}

async fn save_to_database(data: &mut BTreeMap<&str, Vec<Message>>, tether: &mut Tether) {
    let address = create_address(tether).await;
    tether
        .tx::<_, _, StashError>(async |bond| {
            let mut conv = conversation!(remote_id: conv_id!("convid_1"));
            conv.save(bond).await.unwrap();
            for (label_rid, messages) in data.iter_mut() {
                let mut label = label!(remote_id: lbl_id!(label_rid));
                label.save(bond).await.unwrap();
                let mut counters = MessageCounters::new(label.local_id.unwrap());
                counters.total = messages.len() as u64;
                counters.save(bond).await.unwrap();

                for message in messages.iter_mut() {
                    message.local_address_id = address.local_id.unwrap();
                    message.remote_address_id = address.remote_id.clone().unwrap();
                    message.local_conversation_id = conv.local_id;
                    message.remote_conversation_id = conv.remote_id.clone();
                    save_single_message(&label, message, bond).await;
                }
            }
            Ok(())
        })
        .await
        .unwrap();
}

fn expected_messages(
    n: usize,
    label_id: &str,
    data: &BTreeMap<&str, Vec<Message>>,
) -> Option<Vec<Message>> {
    let convs = data.get(label_id)?;
    Some(convs.iter().rev().take(n).cloned().collect())
}

#[tokio::test]
async fn test_message_mail_scroller_reads_correct_items_within_visible_range_for_cached_scroll_data()
 {
    const REMOTE_LABEL_ID: &str = "rid1";
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

    let mut data = btree_map! {
        REMOTE_LABEL_ID: test_message(100, 100),
        "rid2": test_message(50, 0),
    };

    save_to_database(&mut data, &mut tether).await;

    let remote_label_id = LabelId::from(REMOTE_LABEL_ID);
    let local_label_id = Label::resolve_local_label_id(remote_label_id, &tether)
        .await
        .unwrap();
    let unread = ReadFilter::All;
    let last_message = Message::find_by_remote_id(MessageId::from("150"), &tether)
        .await
        .unwrap()
        .unwrap();

    let mut scroller = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_message_id(last_message.remote_id.clone().unwrap())
        .message_time(last_message.time)
        .display_order(last_message.display_order)
        .build();

    tether
        .tx(async |bond| scroller.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;
    let mut scroller =
        MailScroller::messages(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();
    scroller.fetch_more().await.unwrap();
    let actual = scroller.all_items().await.unwrap();
    let expected = expected_messages(page_size, REMOTE_LABEL_ID, &data).unwrap();

    assert_eq!(actual, expected);
    assert!(scroller.has_more().await.unwrap());

    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), page_size);

    let actual = scroller.all_items().await.unwrap();
    let expected = expected_messages(page_size * 2, REMOTE_LABEL_ID, &data).unwrap();

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_message_mail_scroller_reads_one_item_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();
    let message = api_message_meta!(
        id: MessageId::from("mymsg"),
        conversation_id: conversation.id,
        address_id: address.id,
        label_ids: vec![SystemLabel::Inbox.remote_id()]
    );

    ctx.mock_get_messages(vec![message]).await;
    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;

    let page_size = 5;
    let mut scroller =
        MailScroller::messages(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    let expected = scroller.fetch_more().await.unwrap();
    let mut actual = scroller.all_items().await.unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 1);
    let actual = actual.pop().unwrap();
    assert_eq!(actual.remote_id, msg_id!("mymsg"));
    assert!(!scroller.has_more().await.unwrap());

    let next_page = scroller.fetch_more().await.unwrap();
    assert_eq!(next_page.len(), 0);
}

#[tokio::test]
async fn test_message_mail_scroller_reads_two_pages_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    // mocks
    mock_api_sync_prevous_messages_page(&ctx, "mymsg_9", 1).await;
    let params = setup_api_message_pages(&ctx, page_size, 3).await;

    ctx.setup_user(params.clone()).await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Update the inbox label to have all messages
    let mut counters = MessageCounters::load(local_label_id, &tether)
        .await
        .unwrap()
        .unwrap();
    counters.total = page_size as u64 * 2;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    // Online
    let mut scroller =
        MailScroller::messages(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();
    scroller.fetch_more().await.unwrap();
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 5);

    let actual_rids = actual.iter().map(|msg| msg.remote_id.clone()).collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_9"),
            msg_id!("mymsg_8"),
            msg_id!("mymsg_7"),
            msg_id!("mymsg_6"),
            msg_id!("mymsg_5"),
        ]
    );
    assert!(scroller.has_more().await.unwrap());

    // Get next page - it will progress cursor to the next page
    // But there is no more data available, the request will return an empty page
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 5);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 10);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_9"),
            msg_id!("mymsg_8"),
            msg_id!("mymsg_7"),
            msg_id!("mymsg_6"),
            msg_id!("mymsg_5"),
            msg_id!("mymsg_4"),
            msg_id!("mymsg_3"),
            msg_id!("mymsg_2"),
            msg_id!("mymsg_1"),
            msg_id!("mymsg_0"),
        ]
    );
    assert!(!scroller.has_more().await.unwrap());

    // Cached - it will trigger two more background requests for pages as we fetch more
    // This is because cursor have only two pages in cache, which means we will try to get new page everytime we progress

    let mut scroller =
        MailScroller::messages(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();
    scroller.fetch_more().await.unwrap();

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 5);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_9"),
            msg_id!("mymsg_8"),
            msg_id!("mymsg_7"),
            msg_id!("mymsg_6"),
            msg_id!("mymsg_5"),
        ]
    );
    assert!(scroller.has_more().await.unwrap());

    scroller.fetch_more().await.unwrap();
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 10);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_9"),
            msg_id!("mymsg_8"),
            msg_id!("mymsg_7"),
            msg_id!("mymsg_6"),
            msg_id!("mymsg_5"),
            msg_id!("mymsg_4"),
            msg_id!("mymsg_3"),
            msg_id!("mymsg_2"),
            msg_id!("mymsg_1"),
            msg_id!("mymsg_0"),
        ]
    );
    assert!(!scroller.has_more().await.unwrap());
}

#[tokio::test]
async fn test_message_mail_scroller_notificate_about_changes() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let params = setup_api_message_pages(&ctx, page_size, 2).await;

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Update the inbox label to have all messages
    let mut counters = MessageCounters::load(local_label_id, &tether)
        .await
        .unwrap()
        .unwrap();
    counters.total = page_size as u64 * 2;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut scroller =
        MailScroller::messages(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();
    let WatcherHandle {
        handle: _handle,
        receiver,
        ..
    } = scroller.watch().await.unwrap();
    // Setting scroller up will never push notification
    assert!(receiver.is_empty());

    scroller.fetch_more().await.unwrap();

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 5);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_9"),
            msg_id!("mymsg_8"),
            msg_id!("mymsg_7"),
            msg_id!("mymsg_6"),
            msg_id!("mymsg_5"),
        ]
    );
    // Fetching more will never trigger any notifications
    assert!(receiver.is_empty());

    // Get next page
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 5);
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 0);
    // Fetching more will never trigger any notifications
    assert!(receiver.is_empty());

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 10);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_9"),
            msg_id!("mymsg_8"),
            msg_id!("mymsg_7"),
            msg_id!("mymsg_6"),
            msg_id!("mymsg_5"),
            msg_id!("mymsg_4"),
            msg_id!("mymsg_3"),
            msg_id!("mymsg_2"),
            msg_id!("mymsg_1"),
            msg_id!("mymsg_0"),
        ]
    );

    // Lets create a new message and check if it is added to the scroller
    let conversation = params.conversations.first().cloned().unwrap();
    let conversation = Conversation::find_by_remote_id(conversation.id, &tether)
        .await
        .unwrap()
        .unwrap();
    let address = params.addresses.first().cloned().unwrap();
    let address = Address::find_by_remote_id(address.id, &tether)
        .await
        .unwrap()
        .unwrap();
    let test_message = message!(
        remote_id: msg_id!("mymsg_100"),
        local_conversation_id: conversation.local_id,
        remote_conversation_id: conversation.remote_id,
        local_address_id: address.local_id.unwrap(),
        remote_address_id: address.remote_id.unwrap(),
        label_ids: vec![SystemLabel::Inbox.remote_id()],
        display_order: 100,
        time: 100
    );

    tether
        .tx::<_, _, StashError>(async |bond| {
            let label = Label::load(local_label_id, bond).await.unwrap().unwrap();
            save_single_message(&label, &mut test_message.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();
    // Getting an update will trigger a notification
    receiver.recv_async().await.unwrap();

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 11);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_100"),
            msg_id!("mymsg_9"),
            msg_id!("mymsg_8"),
            msg_id!("mymsg_7"),
            msg_id!("mymsg_6"),
            msg_id!("mymsg_5"),
            msg_id!("mymsg_4"),
            msg_id!("mymsg_3"),
            msg_id!("mymsg_2"),
            msg_id!("mymsg_1"),
            msg_id!("mymsg_0"),
        ]
    );
}

async fn setup_api_message_pages(
    ctx: &MailTestContext,
    page_size: usize,
    empty_pages_requests: u64,
) -> TestParams {
    ctx.mock_ping_success().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();
    let test_message = api_message_meta!(
        id: MessageId::from("mymsg"),
        conversation_id: conversation.id,
        address_id: address.id,
        label_ids: vec![SystemLabel::Inbox.remote_id()]
    );

    // Messages are returned and displayed in DESC order, newer at the top
    let second_page = (0..page_size)
        .rev()
        .map(|i| {
            let mut new = test_message.clone();
            new.id = format!("{}_{}", new.id, i).into();
            new.order = i as u64;
            new
        })
        .collect_vec();
    let first_page = (page_size..(page_size * 2))
        .rev()
        .map(|i| {
            let mut new = test_message.clone();
            new.id = format!("{}_{}", new.id, i).into();
            new.order = i as u64;
            new
        })
        .collect_vec();
    let first_page_last_id = first_page.last().map(|conv| conv.id.to_string()).unwrap();
    let second_page_last_id = second_page.last().map(|conv| conv.id.to_string()).unwrap();

    mock_get_messages_page(ctx, second_page, &first_page_last_id, 1_u64).await;
    // last page is empty
    mock_get_messages_page(ctx, vec![], &second_page_last_id, empty_pages_requests).await;
    ctx.mock_get_messages(first_page).await;

    params
}

#[function_name::named]
pub async fn mock_api_sync_prevous_messages_page(
    ctx: &MailTestContext,
    first_id: &str,
    expect: u64,
) {
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/messages"))
        .and(query_param_contains("BeginID", first_id))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                total: 0,
                messages: vec![],
                stale: false,
            }),
        )
        .expect(expect)
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;
}

#[function_name::named]
pub async fn mock_get_messages_page(
    ctx: &MailTestContext,
    messages: Vec<ApiMessageMetadata>,
    last_id: &str,
    expect: u64,
) {
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/messages"))
        .and(query_param_contains("EndID", last_id))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                total: messages.len() as u64,
                messages,
                stale: false,
            }),
        )
        .expect(expect)
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;
}
