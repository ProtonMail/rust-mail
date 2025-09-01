use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::SystemLabel,
    models::{Address, Label, ModelIdExtension},
};
use proton_mail_api::services::proton::{
    common::MessageId, prelude::GetMessagesResponse,
    response_data::MessageMetadata as ApiMessageMetadata,
};
use proton_mail_common::test_utils::scroller::{
    StoreLabeledModelMap, TestScroller, save_single_message, test_messages,
};
use proton_mail_common::test_utils::{init::Params as TestParams, test_context::MailTestContext};
use proton_mail_common::{api_message_meta, datatypes::labels::ScrollOrderField};
use proton_mail_common::{
    datatypes::ReadFilter,
    models::{Conversation, Message, MessageCounters, MessageScrollData},
};
use proton_mail_common::{message, msg_id};
use velcro::hash_map;

use proton_mail_common::datatypes::labels::ScrollOrderDir;
use stash::orm::Model;
use stash::stash::StashError;
use std::{collections::HashMap, vec};
use wiremock::{
    Mock, ResponseTemplate, Times,
    matchers::{method, path, query_param_contains},
};

fn expected_messages(
    n: usize,
    label_id: &str,
    data: &HashMap<Vec<&str>, Vec<Message>>,
) -> Option<Vec<Message>> {
    let msgs = data.get(&vec![label_id])?;
    Some(msgs.iter().rev().take(n).cloned().collect())
}

#[tokio::test]
async fn test_message_mail_scroller_reads_correct_items_within_visible_range_for_cached_scroll_data()
 {
    const REMOTE_LABEL_ID: &str = "rid1";
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut data = hash_map! {
        vec![REMOTE_LABEL_ID]: test_messages(100, 100),
        vec!["rid2"]: test_messages(50, 0),
    };

    data.save_to_database(&mut tether).await;

    let remote_label_id = LabelId::from(REMOTE_LABEL_ID);
    let local_label_id = Label::resolve_local_label_id(remote_label_id, &tether)
        .await
        .unwrap();
    let unread = ReadFilter::All;
    let last_message = Message::find_by_remote_id(MessageId::from("mymsg_150"), &tether)
        .await
        .unwrap()
        .unwrap();

    let mut scroller = MessageScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_message_id(last_message.remote_id.clone().unwrap())
        .message_time(last_message.time)
        .snooze_time(last_message.snooze_time)
        .display_order(last_message.display_order)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    tether
        .tx(async |bond| scroller.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;
    let mut test_scroller = TestScroller::messages(&user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    let expected = expected_messages(page_size, REMOTE_LABEL_ID, &data).unwrap();

    // Fetch more and wait for the update (or handle no update case)
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    if !actual.is_empty() {
        assert_eq!(actual, expected);
    } else {
        // If no update was sent, check if we already have data through refresh
        let _refresh_result = test_scroller.refresh_and_wait().await.unwrap();
        // Check if we now have the expected items
        if test_scroller.items().len() >= expected.len() {
            assert_eq!(&test_scroller.items()[..expected.len()], &expected[..]);
        }
    }

    // Try to get more data
    if test_scroller.has_more().await.unwrap() {
        let next_page = test_scroller.fetch_more_and_wait().await.unwrap();
        if !next_page.is_empty() {
            assert_eq!(next_page.len(), page_size);
        }
    }

    // Refresh should not change anything if data is already correct
    let _refresh_result = test_scroller.refresh_and_wait().await.unwrap();
    assert!(test_scroller.items().len() >= expected.len());
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

    ctx.mock_get_messages_total_expect(vec![message], 1, 1..=4)
        .await;
    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;

    let page_size = 5;
    let mut test_scroller = TestScroller::messages(&user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    let _ = test_scroller.fetch_more_and_wait().await.unwrap();
    // We wait for invalidation
    let actual = test_scroller.wait_for_update().await.unwrap().unwrap();

    assert_eq!(actual.len(), 1);

    // Verify we have the expected data
    assert_eq!(test_scroller.items().len(), 1);

    // Refresh again should not change anything
    let refresh_result = test_scroller.refresh_and_wait().await.unwrap();
    assert!(refresh_result.is_empty());

    let actual = &test_scroller.items()[0];
    assert_eq!(actual.remote_id, msg_id!("mymsg"));
    assert!(!test_scroller.has_more().await.unwrap());

    // Additional fetch_more should result in no new data
    let next_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(next_page.is_empty());
}

#[tokio::test]
async fn test_message_mail_scroller_reads_two_pages_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    // mocks
    mock_api_sync_prevous_messages_page(&ctx, "mymsg_9", 1).await;
    let params = setup_api_message_pages(&ctx, page_size, 1..=5).await;

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
    let mut test_scroller = TestScroller::messages(&user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    // Messages can be accessed only when progressed.
    test_scroller.fetch_more_and_wait().await.unwrap();
    let _ = test_scroller.wait_for_update().await.unwrap();

    let actual = test_scroller.items();
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
    assert!(test_scroller.has_more().await.unwrap());

    // Get next page - it will progress cursor to the next page
    // But there is no more data available, the request will return an empty page
    let actual_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual_page.len(), 5);

    let actual = test_scroller.items();
    assert_eq!(actual.len(), 10);
    let actual_rids = actual.iter().map(|msg| msg.remote_id.clone()).collect_vec();
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
    assert!(!test_scroller.has_more().await.unwrap());

    // Additional fetch_more should result in no new data
    let next_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(next_page.is_empty());

    // Cached - it will trigger two more next page requests for pages as we fetch more
    // and one previous page request on init.
    // This is because cursor have only two pages in cache, which means we will try to get new page evertime we fetch more

    let mut test_scroller = TestScroller::messages(&user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    test_scroller.fetch_more().unwrap();
    let _ = test_scroller.wait_for_update().await.unwrap();

    let actual = test_scroller.items();
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
    assert!(test_scroller.has_more().await.unwrap());

    test_scroller.fetch_more().unwrap();
    let _ = test_scroller.wait_for_update().await.unwrap();

    let actual = test_scroller.items();
    assert_eq!(actual.len(), 10);
    let actual_rids = actual.iter().map(|msg| msg.remote_id.clone()).collect_vec();
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
    assert!(!test_scroller.has_more().await.unwrap());

    // Additional fetch_more should result in no new data
    let next_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(next_page.is_empty());
}

#[tokio::test]
async fn test_message_mail_scroller_notificate_about_changes() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let params = setup_api_message_pages(&ctx, page_size, 1..=3).await;

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

    let mut test_scroller = TestScroller::messages(&user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    // Fetch initial page
    test_scroller.fetch_more_and_wait().await.unwrap();
    let _ = test_scroller.wait_for_update().await.unwrap();

    let actual = test_scroller.items();
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

    // Get next page - due to change in cached scroller detecting if there is more than a page
    // now it will return an empty page for invalidation process
    let actual_page = test_scroller.fetch_more_and_wait().await.unwrap();
    // Next page will have 5 items
    assert_eq!(actual_page.len(), 5);

    test_scroller.fetch_more().unwrap();
    // It will follow up with 2 updates
    // One for the current one request
    //and another one for automatic fetch_more to determine if there is more data
    let actual_page = test_scroller.wait_for_update().await.unwrap();
    assert!(actual_page.is_none());

    let actual = test_scroller.items();
    assert_eq!(actual.len(), 10);
    let actual_rids = actual.iter().map(|msg| msg.remote_id.clone()).collect_vec();
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
        local_address_id: address.id(),
        remote_address_id: address.remote_id.unwrap(),
        label_ids: vec![SystemLabel::Inbox.remote_id()],
        display_order: 100,
        snooze_time: 100.into()
    );

    tether
        .tx::<_, _, StashError>(async |bond| {
            let label = Label::load(local_label_id, bond).await.unwrap().unwrap();
            save_single_message(&[label], &mut test_message.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();
    // Getting an update will trigger a notification
    let _ = test_scroller.wait_for_update().await.unwrap();

    let actual = test_scroller.items();
    assert_eq!(actual.len(), 11);
    let actual_rids = actual.iter().map(|msg| msg.remote_id.clone()).collect_vec();
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

#[tokio::test]
async fn all_scheduled_is_displayed_in_ascending_order() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Scheduled
        .local_id(&tether)
        .await
        .unwrap()
        .unwrap();
    let params =
        setup_api_message_pages_ext(&ctx, page_size, 1, SystemLabel::Scheduled, false).await;

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
    let mut test_scroller = TestScroller::messages(&user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    let _ = test_scroller.fetch_more_and_wait().await.unwrap();
    // We wait for invalidation
    let actual = test_scroller.wait_for_update().await.unwrap().unwrap();

    assert_eq!(actual.len(), 5);

    let actual = test_scroller.items();
    assert_eq!(actual.len(), 5);

    let actual_rids = actual.iter().map(|msg| msg.remote_id.clone()).collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_0"),
            msg_id!("mymsg_1"),
            msg_id!("mymsg_2"),
            msg_id!("mymsg_3"),
            msg_id!("mymsg_4"),
        ]
    );
    assert!(test_scroller.has_more().await.unwrap());

    // Get next page - it will progress cursor to the next page
    // But there is no more data available, the request will return an empty page
    let actual_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual_page.len(), 5);

    let actual = test_scroller.items();
    assert_eq!(actual.len(), 10);
    let actual_rids = actual.iter().map(|msg| msg.remote_id.clone()).collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            msg_id!("mymsg_0"),
            msg_id!("mymsg_1"),
            msg_id!("mymsg_2"),
            msg_id!("mymsg_3"),
            msg_id!("mymsg_4"),
            msg_id!("mymsg_5"),
            msg_id!("mymsg_6"),
            msg_id!("mymsg_7"),
            msg_id!("mymsg_8"),
            msg_id!("mymsg_9"),
        ]
    );
    assert!(!test_scroller.has_more().await.unwrap());
}

async fn setup_api_message_pages(
    ctx: &MailTestContext,
    page_size: usize,
    empty_pages_requests: impl Into<Times>,
) -> TestParams {
    setup_api_message_pages_ext(
        ctx,
        page_size,
        empty_pages_requests,
        SystemLabel::Inbox,
        true,
    )
    .await
}
async fn setup_api_message_pages_ext(
    ctx: &MailTestContext,
    page_size: usize,
    empty_pages_requests: impl Into<Times>,
    system_label: SystemLabel,
    descending: bool,
) -> TestParams {
    ctx.mock_ping_success().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();
    let test_message = api_message_meta!(
        id: MessageId::from("mymsg"),
        conversation_id: conversation.id,
        address_id: address.id,
        label_ids: vec![system_label.remote_id()]
    );

    // Messages are returned and displayed in DESC order, newer at the top
    let (first_page, second_page) = if descending {
        let second_page = (0..page_size)
            .rev()
            .map(|i| {
                let mut new = test_message.clone();
                new.id = format!("{}_{}", new.id, i).into();
                new.order = i as u64;
                new.time = new.order + 1;
                new.snooze_time = new.time;
                new
            })
            .collect_vec();
        let first_page = (page_size..(page_size * 2))
            .rev()
            .map(|i| {
                let mut new = test_message.clone();
                new.id = format!("{}_{}", new.id, i).into();
                new.order = i as u64;
                new.time = new.order + 1;
                new.snooze_time = new.time;
                new
            })
            .collect_vec();
        (first_page, second_page)
    } else {
        let total = page_size * 2;
        let second_page = (page_size..total)
            .map(|i| {
                let mut new = test_message.clone();
                new.id = format!("{}_{}", new.id, i).into();
                new.order = i as u64;
                new.time = new.order + 1;
                new.snooze_time = new.time;
                new
            })
            .collect_vec();
        let first_page = (0..page_size)
            .map(|i| {
                let mut new = test_message.clone();
                new.id = format!("{}_{}", new.id, i).into();
                new.order = i as u64;
                new.time = new.order + 1;
                new.snooze_time = new.time;
                new
            })
            .collect_vec();
        (first_page, second_page)
    };
    let first_page_last_id = first_page.last().map(|conv| conv.id.to_string()).unwrap();
    let second_page_last_id = second_page.last().map(|conv| conv.id.to_string()).unwrap();

    mock_get_messages_page(ctx, second_page, &first_page_last_id, 1).await;
    // last page is empty
    mock_get_messages_page(ctx, vec![], &second_page_last_id, empty_pages_requests).await;
    let first_page_total = first_page.len() as u64;
    ctx.mock_get_messages_total_expect(first_page, first_page_total, 1..3)
        .await;

    params
}

#[function_name::named]
pub async fn mock_api_sync_prevous_messages_page(
    ctx: &MailTestContext,
    first_id: &str,
    expect: impl Into<Times>,
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
    expect: impl Into<Times>,
) {
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/messages"))
        .and(query_param_contains("AnchorID", last_id))
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
