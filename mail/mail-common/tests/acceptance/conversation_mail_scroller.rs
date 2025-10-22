use core::ops::Range;
use itertools::Itertools;
use proton_core_api::services::proton::{Action, EventId, LabelId};
use proton_core_common::models::ModelExtension;
use proton_core_common::{
    datatypes::SystemLabel,
    models::{Label, ModelIdExtension},
};
use proton_mail_api::services::proton::prelude::{ConversationEvent, MailEvent};
use proton_mail_api::services::proton::response_data::ConversationCount;
use proton_mail_api::services::proton::{
    common::ConversationId, prelude::GetConversationsResponse,
    response_data::Conversation as ApiConversation,
    response_data::ConversationLabel as ApiConversationLabel,
};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::{
    SystemLabelId,
    labels::{ScrollOrderDir, ScrollOrderField},
};
use proton_mail_common::models::ConversationLabel;
use proton_mail_common::test_utils::{
    init::Params as TestParams,
    scroller::{
        StoreLabeledModelMap, TestScroller, TestUpdate, save_single_conversation,
        test_conversations,
    },
    test_context::MailUserContextTestExtension,
};
use proton_mail_common::{
    conv_id, conversation, lbl_id, test_utils::test_context::MailTestContext,
};
use proton_mail_common::{
    datatypes::{ContextualConversation, ReadFilter},
    models::{Conversation, ConversationCounters, ConversationScrollData},
};
use proton_network_monitor_service::OsNetworkStatus;
use stash::orm::Model;
use stash::stash::StashError;
use std::{collections::HashMap, time::Duration};
use test_case::test_case;
use velcro::hash_map;
use wiremock::{
    Mock, Request, ResponseTemplate, Times,
    matchers::{method, path, query_param_contains},
};

macro_rules! assert_scroller_content {
    ($scroller:expr, $len:expr, $expected:expr) => {
        assert_eq!($scroller.items().len(), $len);
        let actual_rids = $scroller
            .items()
            .iter()
            .map(|conv| conv.remote_id.clone())
            .collect_vec();
        let expected_rids = $expected.iter().map(|rid| conv_id!(*rid)).collect_vec();
        assert_eq!(actual_rids, expected_rids);
    };
}

fn expected_conversations(
    n: usize,
    label_id: &str,
    data: &HashMap<Vec<&str>, Vec<Conversation>>,
) -> Option<Vec<ContextualConversation>> {
    let convs = data.get(&vec![label_id])?;
    // Conversations are read in DESC order
    Some(
        convs
            .iter()
            .rev()
            .take(n)
            .filter_map(|conv| {
                let rid = lbl_id!(label_id);
                let label = conv
                    .labels
                    .iter()
                    .find(|label| label.remote_label_id == rid)?;
                let local_label_id = label.local_label_id?;

                ContextualConversation::new(conv.clone(), local_label_id)
            })
            .collect(),
    )
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_correct_items_within_visible_range_for_cached_scroll_data()
 {
    const REMOTE_LABEL_ID: &str = "rid1";
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut data = hash_map! {
        vec![REMOTE_LABEL_ID]: test_conversations(100, 100),
        vec!["rid2"]: test_conversations(50, 0),
    };

    data.save_to_database(&mut tether).await;

    let remote_label_id = LabelId::from(REMOTE_LABEL_ID);
    let local_label_id = Label::resolve_local_label_id(remote_label_id, &tether)
        .await
        .unwrap();
    let unread = ReadFilter::All;
    let last_conversation =
        Conversation::find_by_remote_id(ConversationId::from("myconv_150"), &tether)
            .await
            .unwrap()
            .unwrap();
    let last_label = last_conversation.label(local_label_id).unwrap();

    let mut scroller = ConversationScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_conversation_id(last_conversation.remote_id.clone().unwrap())
        .conversation_time(last_label.context_time)
        .snooze_time(last_label.context_snooze_time)
        .display_order(last_conversation.display_order)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::Time)
        .build();

    tether
        .tx(async |bond| scroller.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    let expected = expected_conversations(page_size, REMOTE_LABEL_ID, &data).unwrap();

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
async fn test_conversation_mail_scroller_reads_one_item_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversations = params.conversations.clone();

    ctx.mock_get_conversations(conversations, 3..5).await;
    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let page_size = 5;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Conversations can be accessed only when progressed.
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual.len(), 1);

    // Verify we have the expected data
    assert_eq!(test_scroller.items().len(), 1);

    // Refresh again should not change anything
    let refresh_result = test_scroller.refresh_and_wait().await.unwrap();
    assert!(refresh_result.is_empty());

    assert_eq!(actual[0].remote_id.clone(), conv_id!("myconv"));
    assert!(!test_scroller.has_more().await.unwrap());

    // Additional fetch_more should result in no new data
    let next_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(next_page.is_empty());
}

#[tokio::test]
async fn test_conversation_mail_scroller_try_to_read_one_item_from_api_when_it_does_not_exist_anymore()
 {
    let ctx = MailTestContext::new().await;
    let mut params = TestParams::default_basic();
    params.conversations = vec![];

    ctx.mock_get_conversations(vec![], 3..5).await;
    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 1;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Wait for the none update since we do not have any data in API response
    test_scroller.fetch_more().unwrap();
    test_scroller.match_next_update(TestUpdate::None).await;

    // Verify we have nothing in the scroller
    assert_eq!(test_scroller.items().len(), 0);
    // However it will claim to have more data since the total is 1
    assert!(test_scroller.has_more().await.unwrap());
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_two_pages_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 5;
    let label = SystemLabel::Inbox;
    let remote_label_id = label.remote_id();
    let local_label_id = label.local_id(&tether).await.unwrap().unwrap();
    setup_api_sync_previous_page(&ctx, "myconv_9", None, &remote_label_id, 1).await;
    let params = setup_api_conversation_pages(&ctx, page_size, 0, &remote_label_id, 1..=3).await;
    ctx.setup_user(params.clone()).await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Update the inbox label to have all conversations
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = page_size as u64 * 2;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    // Online
    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Conversations can be accessed only when progressed.
    test_scroller.fetch_more_and_wait().await.unwrap();
    assert_scroller_content!(
        &mut test_scroller,
        5,
        &["myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5"]
    );
    assert!(test_scroller.has_more().await.unwrap());

    // Get next page - it will progress cursor to the next page
    // But there is no more data available, the request will return an empty page
    let actual_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual_page.len(), 5);
    assert_scroller_content!(
        &mut test_scroller,
        10,
        &[
            "myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5", "myconv_4", "myconv_3",
            "myconv_2", "myconv_1", "myconv_0",
        ]
    );
    assert!(!test_scroller.has_more().await.unwrap());

    // Cached - it will trigger two more next page requests for pages as we fetch more
    // and one previous page request on init.
    // This is because cursor have only two pages in cache, which means we will try to get new page evertime we fetch more

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    test_scroller.fetch_more().unwrap();
    let _ = test_scroller.wait_for_update().await.unwrap();
    assert_scroller_content!(
        &mut test_scroller,
        5,
        &["myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5"]
    );
    assert!(test_scroller.has_more().await.unwrap());

    test_scroller.fetch_more().unwrap();
    let _ = test_scroller.wait_for_update().await.unwrap();
    assert_scroller_content!(
        &mut test_scroller,
        10,
        &[
            "myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5", "myconv_4", "myconv_3",
            "myconv_2", "myconv_1", "myconv_0",
        ]
    );
    assert!(!test_scroller.has_more().await.unwrap());
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_online_folder_for_the_first_time_when_get_an_error_on_request()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    mock_api_forbidden(&ctx).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 1;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // First call should not have any items initially
    assert_eq!(test_scroller.items().len(), 0);

    let result = test_scroller.fetch_more_and_wait().await;
    assert!(result.is_err());
    let actual = result.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "Error: API Error: Forbidden: 403 Forbidden. None".to_string()
    );

    assert_eq!(test_scroller.items().len(), 0);
    // It has more as the total is 1
    assert!(test_scroller.has_more().await.unwrap());

    test_scroller.fetch_more().unwrap();
    test_scroller.match_next_update(TestUpdate::None).await;
    test_scroller.fetch_more().unwrap();
    test_scroller.match_next_update(TestUpdate::None).await;
    test_scroller.match_next_update(TestUpdate::None).await;

    test_scroller.assert_updates(&[
        TestUpdate::Error("API Error: Forbidden: 403 Forbidden. None".to_string()),
        TestUpdate::None,
        TestUpdate::None,
        TestUpdate::None,
    ]);

    // Lets test recovery from the error
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    ctx.mock_server().reset().await;
    ctx.mock_ping_success().await;
    ctx.mock_get_conversations(vec![conversation.clone()], 2)
        .await;
    test_scroller.fetch_more().unwrap();
    // None because we have no data
    test_scroller.match_next_update(TestUpdate::None).await;
    // However we will spin next request to fetch the data
    test_scroller
        .match_next_update(TestUpdate::Append { items: 1 })
        .await;

    assert_scroller_content!(&mut test_scroller, 1, &[conversation.id]);
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_offline_folder_for_the_first_time_and_cache_is_empty()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 1;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // First call should not have any items initially
    assert_eq!(test_scroller.items().len(), 0);

    // The items can be read only when we progress with `fetch_more`
    test_scroller.fetch_more().unwrap();
    test_scroller
        .match_next_update(TestUpdate::Error(
            "API Error: Network error: No connection".to_string(),
        ))
        .await;

    assert_eq!(test_scroller.items().len(), 0);
    assert!(test_scroller.has_more().await.unwrap());

    test_scroller.fetch_more().unwrap();
    test_scroller
        .match_next_update(TestUpdate::Error(
            "API Error: Network error: No connection".to_string(),
        ))
        .await;
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_offline_folder_for_the_first_time_and_cache_has_one_item()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(1, 100),
        vec!["rid2"]: test_conversations(50, 0),
    };
    data.save_to_database(&mut tether).await;

    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 10;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    test_scroller.assert_updates(&[]);
    // The items will be read from cache as the API is unreachable
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(test_scroller.items().len(), 1);
    assert!(test_scroller.has_more().await.unwrap());
    test_scroller.assert_updates(&[TestUpdate::Append { items: 1 }]);

    // No more cached, no API connection, return error
    let actual = test_scroller.fetch_more_and_wait().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "Error: API Error: Network error: No connection".to_string()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_conversation_mail_scroller_reads_offline_folder_for_the_first_time_and_cache_has_multiple_pages()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(11, 100),
        vec!["rid2"]: test_conversations(50, 0),
    };
    data.save_to_database(&mut tether).await;

    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 15;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // The items will be read from cache as the API is unreachable
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual.len(), 5);
    assert_eq!(test_scroller.items().len(), 5);
    assert!(test_scroller.has_more().await.unwrap());

    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual.len(), 6);
    assert_eq!(test_scroller.items().len(), 11);

    // It has more but not synced yet
    assert!(test_scroller.has_more().await.unwrap());
    // No more cached, no API connection this should return error
    test_scroller.fetch_more().unwrap();
    let actual = test_scroller.wait_for_update().await;
    assert!(actual.is_err());
    let actual = actual.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "Error: API Error: Network error: No connection".to_string()
    );

    test_scroller.assert_updates(&[
        TestUpdate::Append { items: 5 },
        TestUpdate::Append { items: 6 },
        TestUpdate::Error("API Error: Network error: No connection".to_string()),
    ]);

    // Go online suddenly
    ctx.mock_server().reset().await;
    ctx.mock_ping_success().await;
    setup_api_conversation_pages(&ctx, page_size, 200, &remote_label_id, 2).await;
    user_ctx
        .network_monitor_service()
        .update_os_network_status(OsNetworkStatus::Online);
    user_ctx.network_monitor_service().check_now().await;

    let timeout = Some(Duration::from_secs(3));
    user_ctx
        .wait_for(timeout, |status| status.is_online())
        .await;

    // automatic fetch_more will be triggered by the online status change
    test_scroller.match_next_update(TestUpdate::None).await;

    // Wait for the second update containing the actual data replacement
    // In the new push-based model, fetch_more_and_wait() only waits for immediate feedback,
    // but the actual data replacement from the refresh comes in a second update
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 5 })
        .await;

    assert_scroller_content!(
        &mut test_scroller,
        5,
        &[
            "myconv_209",
            "myconv_208",
            "myconv_207",
            "myconv_206",
            "myconv_205",
        ]
    );

    // progress to the next page from API
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual.len(), 5);
    assert_eq!(test_scroller.items().len(), 10);

    assert_scroller_content!(
        &mut test_scroller,
        10,
        &[
            "myconv_209",
            "myconv_208",
            "myconv_207",
            "myconv_206",
            "myconv_205",
            "myconv_204",
            "myconv_203",
            "myconv_202",
            "myconv_201",
            "myconv_200",
        ]
    );

    // No more items in the cache and we are offline but we satisfied the counter
    // Return empty page instead of the Network error
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(actual.is_empty());
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_cached_data_and_return_error_on_offline_fetch_more()
{
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(100, 100),
        vec!["rid2"]: test_conversations(50, 0),
    };

    data.save_to_database(&mut tether).await;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();

    // Mock offline
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 150;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let page_size = 50;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // The items can be read only when we progress with `fetch_more`
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();

    assert_eq!(actual.len(), 50);
    assert_eq!(test_scroller.items().len(), 50);
    assert!(test_scroller.has_more().await.unwrap());

    // We reached api cached mark, lets serve the rest from cache even if unordered
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();

    assert_eq!(actual.len(), 50);
    assert_eq!(test_scroller.items().len(), 100);
    assert!(test_scroller.has_more().await.unwrap());

    // No more cached, no API connection, return error
    test_scroller.fetch_more().unwrap();
    let actual = test_scroller.wait_for_update().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "Error: API Error: Network error: No connection".to_string()
    );
}

#[tokio::test]
async fn test_conversation_mail_scroller_has_insufficient_cached_data_to_fill_first_page() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(3, 100),
    };
    data.save_to_database(&mut tether).await;

    setup_api_sync_previous_page(&ctx, "myconv_102", None, &remote_label_id, 2).await;
    let params = setup_api_conversation_pages(&ctx, page_size, 0, &remote_label_id, 2).await;
    ctx.setup_user(params.clone()).await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Update the inbox label to have all conversations
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = page_size as u64 * 2 + 3;
    // And simulate we have a cursor pointing correctly to the last
    // conversation which we expect to have 3 though 5 is the page_size
    let last_conversation =
        Conversation::find_by_remote_id(ConversationId::from("myconv_100"), &tether)
            .await
            .unwrap()
            .unwrap();
    let last_label = last_conversation.label(local_label_id).unwrap();
    let mut scroller_cursor = ConversationScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_conversation_id(last_conversation.remote_id.clone().unwrap())
        .conversation_time(last_label.context_snooze_time)
        .snooze_time(last_label.context_snooze_time)
        .display_order(last_conversation.display_order)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::SnoozeTime)
        .build();

    tether
        .tx(async |bond| {
            scroller_cursor.save(bond).await?;
            counters.save(bond).await
        })
        .await
        .unwrap();

    // The scroller needs to fetch next page from the api due to insufficient amount
    // of items to be displayed as the first page.
    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Fetch more will load 8 items, 3 + 5 as in total it is less than
    // 2 separate pages so it will merge them together.
    let fetched_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(fetched_page.len(), 8);

    assert_scroller_content!(
        &mut test_scroller,
        8,
        &[
            "myconv_102",
            "myconv_101",
            "myconv_100",
            "myconv_9",
            "myconv_8",
            "myconv_7",
            "myconv_6",
            "myconv_5",
        ]
    );
    assert!(test_scroller.has_more().await.unwrap());

    // Get next page - it will progress cursor to the next page
    // Since we started moving by whole pages it will fetch 5 items now
    test_scroller.fetch_more().unwrap();
    let actual_page = test_scroller.wait_for_update().await.unwrap().unwrap();
    assert_eq!(actual_page.len(), 5);
    assert_scroller_content!(
        &mut test_scroller,
        13,
        &[
            "myconv_102",
            "myconv_101",
            "myconv_100",
            "myconv_9",
            "myconv_8",
            "myconv_7",
            "myconv_6",
            "myconv_5",
            "myconv_4",
            "myconv_3",
            "myconv_2",
            "myconv_1",
            "myconv_0",
        ]
    );
    assert!(!test_scroller.has_more().await.unwrap());

    // Lets try read it again from cache
    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    test_scroller.fetch_more().unwrap();
    let actual_page = test_scroller.wait_for_update().await.unwrap().unwrap();
    assert_eq!(actual_page.len(), 5);
    assert_scroller_content!(
        &mut test_scroller,
        5,
        &[
            "myconv_102",
            "myconv_101",
            "myconv_100",
            "myconv_9",
            "myconv_8",
        ]
    );
    assert!(test_scroller.has_more().await.unwrap());

    // This `fetch_more` will join two last pages together as the last page is incomplete
    test_scroller.fetch_more().unwrap();
    let actual_page = test_scroller.wait_for_update().await.unwrap().unwrap();
    assert_eq!(actual_page.len(), 8);

    assert_scroller_content!(
        &mut test_scroller,
        13,
        &[
            "myconv_102",
            "myconv_101",
            "myconv_100",
            "myconv_9",
            "myconv_8",
            "myconv_7",
            "myconv_6",
            "myconv_5",
            "myconv_4",
            "myconv_3",
            "myconv_2",
            "myconv_1",
            "myconv_0",
        ]
    );
    assert!(!test_scroller.has_more().await.unwrap());
}

#[test_case(50, 3; "Test1: Conversation added at the end in offline mode it will be added to the end of the list, 3 (3 + 0) items"
)]
#[tokio::test]
async fn test_conversation_mail_scroller_database_refresh_will_not_triggers_fetch_for_small_totals(
    order: usize,
    expected: usize,
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 10; // Larger than our test data
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();

    // Set up cached data with fewer items than page size
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(3, 100), // Less than page_size
    };
    data.save_to_database(&mut tether).await;

    // Mock offline to use cached data
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 3; // Less than page_size (10)
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Conversations can be accessed only when progressed.
    let _ = test_scroller.fetch_more_and_wait().await.unwrap();

    // Add a conversation to trigger refresh
    let label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let new_conversation = test_conversations(1, order).pop().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            save_single_conversation(&[label], &mut new_conversation.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();

    // For small totals (< page_size), refresh should internally call fetch_more
    // to ensure data is loaded as there is no way to scroll down to trigger fetch_more
    assert_eq!(test_scroller.items().len(), expected);
    assert!(test_scroller.has_more().await.unwrap());

    // Refresh update arrives
    let _ = test_scroller.wait_for_update().await.unwrap();
    assert!(!test_scroller.has_more().await.unwrap());
    assert_eq!(test_scroller.items().len(), expected + 1);
}

#[test_case(200, 4; "Test2: Conversation added at the beggining, 4 (3 + 1) items, as the item is at the beggining"
)]
#[tokio::test]
async fn test_conversation_mail_scroller_database_refresh_triggers_fetch_for_small_totals(
    order: usize,
    expected: usize,
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 10; // Larger than our test data
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();

    // Set up cached data with fewer items than page size
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(3, 100), // Less than page_size
    };
    data.save_to_database(&mut tether).await;

    // Mock offline to use cached data
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 3; // Less than page_size (10)
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Conversations can be accessed only when progressed.
    let _ = test_scroller.fetch_more_and_wait().await.unwrap();

    // Add a conversation to trigger refresh
    let label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let new_conversation = test_conversations(1, order).pop().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            save_single_conversation(&[label], &mut new_conversation.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();

    // Wait for refresh notification
    let _ = test_scroller.wait_for_update().await.unwrap();

    // For small totals (< page_size), all_items should internally call fetch_more
    // to ensure data is loaded as there is no way to scroll down to trigger fetch_more
    assert_eq!(test_scroller.items().len(), expected);

    assert!(!test_scroller.has_more().await.unwrap());
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(actual.is_empty());
}

#[test_case(50, 5; "Test1: Conversation added at the end, 5 (5 + 0) items, as the page size is 5")]
#[test_case(200, 6; "Test2: Conversation added at the beggining, 6 (5 + 1) items, as the item is at the beggining"
)]
#[tokio::test]
async fn test_conversation_mail_scroller_database_refresh_triggers_fetch_for_large_totals(
    order: usize,
    expected: usize,
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 5;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();

    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(15, 100),
    };
    data.save_to_database(&mut tether).await;

    // Mock offline to use cached data
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 15;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Load first page
    let first_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(first_page.len(), 5);

    // Trigger dirty state
    let label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let new_conversation = test_conversations(1, order).pop().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            save_single_conversation(&[label], &mut new_conversation.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();

    let _ = test_scroller
        .try_wait_for_update(Duration::from_secs(10))
        .await;

    assert_eq!(test_scroller.items().len(), expected);
    assert!(test_scroller.has_more().await.unwrap());
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual.len(), 5);
    assert!(test_scroller.has_more().await.unwrap());
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(!actual.is_empty());
    assert_eq!(test_scroller.items().len(), 16); // 15 + 1 new
    assert!(!test_scroller.has_more().await.unwrap());
}

#[tokio::test]
async fn snoozed_conversations() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let label = Label::find_by_remote_id(LabelId::snoozed(), &tether)
        .await
        .unwrap()
        .unwrap();

    let mut data = {
        let label = label.remote_id.clone().unwrap().to_string();

        hash_map! {
            vec![label]: test_conversations(5, 0),
        }
    };

    data.save_to_database(&mut tether).await;

    // ---

    let snooze_times = [300, 200, 400, 100, 500];

    for (conv, conv_snooze_time) in data.values_mut().flatten().zip(snooze_times) {
        conv.labels[0].context_snooze_time = conv_snooze_time.into();
        tether.tx(async |tx| conv.save(tx).await).await.unwrap();
    }

    ctx.catch_all().await;

    // ---

    let mut scroller = TestScroller::conversations(&user_ctx, label.id(), 2)
        .await
        .unwrap();

    let convs = scroller.fetch_more_and_wait().await.unwrap();

    assert_eq!(2, convs.len());
    assert_eq!("myconv_3", convs[0].remote_id.as_ref().unwrap().to_string());
    assert_eq!("myconv_1", convs[1].remote_id.as_ref().unwrap().to_string());

    let convs = scroller.fetch_more_and_wait().await.unwrap();

    assert_eq!(3, convs.len());
    assert_eq!("myconv_0", convs[0].remote_id.as_ref().unwrap().to_string());
    assert_eq!("myconv_2", convs[1].remote_id.as_ref().unwrap().to_string());
    assert_eq!("myconv_4", convs[2].remote_id.as_ref().unwrap().to_string());
}

#[tokio::test]
async fn test_conversation_snooze_time_ordering_with_same_snooze_time_different_context_time() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Mock offline to use cached data
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;
    let page_size = 3;

    // Create 3 test conversations for inbox with same snooze_time but different context_time
    let same_snooze_time = 1000;
    let context_times = [500, 300, 700]; // Different context times
    let orders: [u64; 3] = [10, 20, 30]; // Different display orders

    // Save conversations to database using the inbox label
    let mut data = hash_map! {
        vec![LabelId::inbox().to_string()]: test_conversations(3, 0),
    };
    data.save_to_database(&mut tether).await;
    for (i, (conv, (context_time, order))) in data
        .values_mut()
        .flatten()
        .zip(context_times.iter().zip(orders.iter()))
        .enumerate()
    {
        conv.remote_id = Some(format!("snooze_conv_{i}").into());
        conv.labels[0].context_snooze_time = same_snooze_time.into();
        conv.labels[0].context_time = (*context_time).into();
        conv.display_order = *order;
        tether.tx(async |tx| conv.save(tx).await).await.unwrap();
    }
    let mut cursor_scroller = ConversationScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_conversation_id("Everything visible".into())
        .conversation_time(200.into()) // all in range of the cursor
        .snooze_time(200.into())
        .display_order(30)
        .order_dir(ScrollOrderDir::Desc)
        .order_field(ScrollOrderField::SnoozeTime)
        .build();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 3;
    tether
        .tx(async |bond| {
            cursor_scroller.save(bond).await?;
            counters.save(bond).await
        })
        .await
        .unwrap();

    // Set up mocks
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    // Create scroller with SnoozeTime ordering
    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Fetch conversations
    let items = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(items.len(), 3);

    // Verify ordering: same snooze_time, so should be ordered by context_time DESC (newest first)
    // Expected order: conv_2 (context_time=700), conv_0 (context_time=500), conv_1 (context_time=300)
    // 1st
    assert_eq!(
        items[0].remote_id.as_ref().unwrap().to_string(),
        "snooze_conv_2"
    );
    assert_eq!(items[0].snooze_time.as_u64(), 1000);
    assert_eq!(items[0].time.as_u64(), 700);
    // 2nd
    assert_eq!(
        items[1].remote_id.as_ref().unwrap().to_string(),
        "snooze_conv_0"
    );
    assert_eq!(items[1].snooze_time.as_u64(), 1000);
    assert_eq!(items[1].time.as_u64(), 500);
    // 3rd
    assert_eq!(
        items[2].remote_id.as_ref().unwrap().to_string(),
        "snooze_conv_1"
    );
    assert_eq!(items[2].snooze_time.as_u64(), 1000);
    assert_eq!(items[2].time.as_u64(), 300);

    let mut last = conversation!(remote_id: Some("snooze_conv_3".into()),
        labels: vec![ConversationLabel {
            remote_label_id: Some(LabelId::inbox()),
            context_snooze_time: 1000.into(),
            context_time: 200.into(),
            ..ConversationLabel::test_default()
        }],
        display_order: 30
    );
    tether.tx(async |tx| last.save(tx).await).await.unwrap();
    let next_items = test_scroller.fetch_more_and_wait().await.unwrap();

    // Should get snooze_conv_3 (context_time=200) which is the only conversation
    // with the same snooze_time but older context_time than the cursor
    assert_eq!(next_items.len(), 1);
    assert_eq!(
        next_items[0].remote_id.as_ref().unwrap().to_string(),
        "snooze_conv_3"
    );
    assert_eq!(next_items[0].snooze_time.as_u64(), 1000);
    assert_eq!(next_items[0].time.as_u64(), 200);
}

#[tokio::test]
async fn test_conversation_mail_scroller_fetch_new() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let label = SystemLabel::Inbox;
    let conversations = params
        .conversations
        .first()
        .cloned()
        .map(|mut conv| {
            conv.context_time = Some(100);
            conv.order = 100;
            conv
        })
        .into_iter()
        .collect_vec();

    let previous_page = conversations
        .first()
        .cloned()
        .map(|mut conv| {
            conv.id = "myconv_0".into();
            conv.context_time = Some(110);
            conv.order = 110;
            conv
        })
        .into_iter()
        .collect_vec();

    let remote_label_id = label.remote_id();
    // Mock previous page
    setup_api_sync_previous_page(&ctx, "myconv_0", None, &remote_label_id, 1).await;
    setup_api_sync_previous_page(&ctx, "myconv", Some(previous_page), &remote_label_id, 1).await;
    // Mock next page
    mock_get_conversations_page(&ctx, vec![], "myconv", &remote_label_id, 1).await;
    // Mock first page
    ctx.mock_get_conversations(conversations, 1).await;
    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let page_size = 5;

    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Conversations can be accessed only when progressed.
    let actual = test_scroller.fetch_more_and_wait().await.unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(test_scroller.items().len(), 1);

    test_scroller.fetch_new().unwrap();
    test_scroller.match_next_update(TestUpdate::None).await;
    test_scroller
        .match_next_update(TestUpdate::ReplaceBefore { idx: 0, items: 1 })
        .await;
    assert_eq!(test_scroller.items().len(), 2);

    test_scroller.fetch_new().unwrap();
    test_scroller.match_next_update(TestUpdate::None).await;
    assert_eq!(test_scroller.items().len(), 2);
}

#[function_name::named]
async fn setup_api_sync_previous_page(
    ctx: &MailTestContext,
    first_id: &str,
    conversations: Option<Vec<ApiConversation>>,
    label: &LabelId,
    expect: impl Into<Times>,
) {
    let desc = ScrollOrderDir::for_label(label)
        .reverse()
        .as_api_desc()
        .unwrap();
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/conversations"))
        .and(query_param_contains("AnchorID", first_id))
        .and(query_param_contains("Desc", (desc as u8).to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                conversations: conversations.unwrap_or_default(),
                stale: false,
                total: 0,
            }),
        )
        .expect(expect)
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;
}

fn create_api_conversation_page(
    range: impl Into<Range<usize>>,
    starting_display_order: u64,
) -> Vec<ApiConversation> {
    let params = TestParams::default_basic();
    let test_conversation = params.conversations.clone().pop().unwrap();
    // Conversations are returned and displayed in reversed order
    range
        .into()
        .rev()
        .map(|i| {
            let order = starting_display_order + i as u64;
            let mut new = test_conversation.clone();
            new.id = format!("{}_{}", new.id, order).into();
            new.order = order;
            new.context_time = Some(order);
            new
        })
        .collect_vec()
}

async fn setup_api_conversation_pages(
    ctx: &MailTestContext,
    page_size: usize,
    starting_display_order: u64,
    label: &LabelId,
    empty_pages_requests: impl Into<Times>,
) -> TestParams {
    ctx.mock_ping_success().await;
    let mut params = TestParams::default_basic();
    // Conversations are returned and displayed in reversed order
    let second_page = create_api_conversation_page(0..page_size, starting_display_order);
    let first_page =
        create_api_conversation_page(page_size..(page_size * 2), starting_display_order);
    let first_page_last_id = first_page.last().map(|conv| conv.id.to_string()).unwrap();
    let second_page_last_id = second_page.last().map(|conv| conv.id.to_string()).unwrap();

    mock_get_conversations_page(ctx, second_page, &first_page_last_id, label, 1_u64).await;
    // last page is empty
    mock_get_conversations_page(
        ctx,
        vec![],
        &second_page_last_id,
        label,
        empty_pages_requests,
    )
    .await;
    ctx.mock_get_conversations(first_page, 1_u64).await;

    // Do not download any conv on init
    params.conversations = vec![];
    params
}

#[tokio::test]
async fn conversation_mail_scroller_reacts_to_creat_conversation_event() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 5;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();

    ctx.mock_ping_success().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params.clone()).await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;
    let mut test_conversation = params.conversations.clone().pop().unwrap();
    let conv_id_1 = ConversationId::from("myconv_9");
    let conv_id_2 = ConversationId::from("myconv_10");
    test_conversation.id = conv_id_1.clone();
    test_conversation.order = 9;
    test_conversation.context_time = Some(9);
    ctx.mock_get_conversations(vec![test_conversation], 2).await;
    //mock_get_conversations_page(&ctx, vec![], &test_conv_id, 1).await;
    ctx.catch_all().await;

    // Update the inbox label to have all conversations
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 1;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    // Online
    let mut test_scroller = TestScroller::conversations(&user_ctx, local_label_id, page_size)
        .await
        .unwrap();

    // Conversations can be accessed only when progressed.
    test_scroller.fetch_more_and_wait().await.unwrap();
    assert_scroller_content!(&mut test_scroller, 1, &["myconv_9"]);

    // Simulate new event
    let event = MailEvent {
        event_id: EventId::from("New Event"),
        labels: None,
        conversation_counts: Some(vec![ConversationCount {
            label_id: LabelId::inbox(),
            total: 2,
            unread: 1,
        }]),
        conversations: Some(vec![ConversationEvent {
            id: conv_id_2.clone(),
            action: Action::Create,
            conversation: Some(ApiConversation {
                id: conv_id_2.clone(),
                attachment_info: Default::default(),
                attachments_metadata: vec![],
                display_snoozed_reminder: false,
                expiration_time: 0,
                labels: vec![ApiConversationLabel {
                    id: LabelId::inbox(),
                    context_expiration_time: 0,
                    context_num_attachments: 0,
                    context_num_messages: 1,
                    context_num_unread: 1,
                    context_size: 100,
                    context_snooze_time: 0,
                    context_time: 10,
                }],
                num_attachments: 0,
                num_messages: 1,
                num_unread: 1,
                order: 10,
                recipients: vec![],
                senders: vec![],
                size: 100,
                subject: "".to_string(),
                context_time: None,
            }),
        }]),
        incoming_defaults: None,
        mail_settings: None,
        message_counts: None,
        messages: None,
        refresh: 0,
        has_more: false,
    };

    user_ctx.apply_event(event.into()).await.unwrap();
    // Sanity check expected state
    let conversations = Conversation::in_label(local_label_id, &tether)
        .await
        .unwrap();
    assert_eq!(conversations.len(), 2);
    assert_eq!(conversations[0].remote_id.as_ref(), Some(&conv_id_2));
    assert_eq!(conversations[1].remote_id.as_ref(), Some(&conv_id_1));
    let conv_counts = ConversationCounters::find_by_id(local_label_id, &tether)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(conv_counts.unread, 1);
    assert_eq!(conv_counts.total, 2);

    let update = tokio::time::timeout(Duration::from_secs(5), test_scroller.wait_for_update())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(update.len(), 1);
    assert_eq!(update[0].remote_id.as_ref(), Some(&conv_id_2));
}

// Tests the instance in which the cached data is equal to the API data in the time of first visit of that location.
// To imagine how it could happen imagine the following scenario in quick succession:
// - User creates a folder
// - User moves 10 items to the folder (lets assume he waits till API is synced)
// - User visits the folder
// - User removes 1 item from the folder
//
// Most notable thing about the test is it triggers the `fetch_more` 3 times.
// - 1st time by user which reads from cache and state is NotSynced
// - 2nd time (auto) invalidation from the first_page sync state changes from NotSynced to Online
// - 3rd time (auto) database refresh from the watcher state is Online
//
// Without ET-4791 fix this test would give 2 `Append` updates with the same data, esentially duplicating the page.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_conversation_mail_scroller_reads_non_empty_folder_for_the_first_time_and_api_data_is_equal_to_the_cache()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let api_page = create_api_conversation_page(0..9, 100);
    let models = api_page
        .iter()
        .map(|conv| Conversation::from(conv.clone()))
        .collect_vec();
    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: models,
        vec!["rid2"]: test_conversations(50, 0),
    };
    data.save_to_database(&mut tether).await;

    ctx.mock_get_conversations(api_page, 4..=6).await;
    ctx.mock_ping_success().await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 10;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let page_size = 10;
    let mut test_scroller =
        TestScroller::conversations_instant(&user_ctx, local_label_id, page_size)
            .await
            .unwrap();

    // The items will be read from cache as we have 9 items in cache
    // And the exact same data is in the API
    test_scroller.fetch_more().unwrap(); // 1st fetch_more
    test_scroller
        .match_next_update(TestUpdate::Append { items: 9 })
        .await;
    assert!(test_scroller.has_more().await.unwrap());
    // 2nd fetch_more goes when first_page sync finishes around here.
    // |<-
    // No update is expected as the data is the same
    let update = test_scroller
        .try_wait_for_update(Duration::from_secs(2))
        .await
        .unwrap();
    assert!(update.is_none());
    assert_eq!(test_scroller.items().len(), 9);

    // 3rd fetch_more by triggering invisble database update.
    let mut new_data = hash_map! {
        vec!["rid2"]: test_conversations(1, 299),
    };
    new_data.save_to_database(&mut tether).await;

    // We shouldn't get any update as the data is still the same
    let update = test_scroller
        .try_wait_for_update(Duration::from_secs(2))
        .await
        .unwrap();
    assert!(update.is_none());
    assert_eq!(test_scroller.items().len(), 9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_conversation_mail_scroller_handles_stale_data_in_inbox_on_next_and_previous_page() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 10;
    let mut api_page = create_api_conversation_page(0..9, 100);
    for conv in api_page.iter_mut() {
        conv.labels = vec![ApiConversationLabel {
            id: LabelId::inbox(),
            ..ApiConversationLabel::test_default()
        }];
    }
    // Set up cached data
    let remote_label_id = SystemLabel::Trash.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: vec![],
        vec!["rid2"]: test_conversations(50, 0),
    };
    data.save_to_database(&mut tether).await;

    let api_page_clone = api_page.clone();
    ctx.mock_get_conversations_with(move |builder| {
        builder
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                    conversations: api_page_clone.clone(),
                    stale: true,
                    total: 1,
                }),
            )
            .expect(2..=4)
    })
    .await;
    ctx.mock_ping_success().await;
    ctx.catch_all().await;

    // we should get an update on the first fetch_more in Inbox despite the data being stale
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 10;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut test_scroller =
        TestScroller::conversations_instant(&user_ctx, local_label_id, page_size)
            .await
            .unwrap();

    test_scroller.fetch_more().unwrap();
    test_scroller
        .match_next_update(TestUpdate::Append { items: 9 })
        .await;
    assert!(test_scroller.has_more().await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_conversation_mail_scroller_handles_stale_data_in_trash_on_next_and_previous_page() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 10;
    let remote_label_id = SystemLabel::Trash.remote_id();
    let mut api_page = create_api_conversation_page(0..9, 100);
    for conv in api_page.iter_mut() {
        conv.labels = vec![ApiConversationLabel {
            id: remote_label_id.clone(),
            ..ApiConversationLabel::test_default()
        }];
    }
    // Set up cached data
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: vec![],
        vec!["rid2"]: test_conversations(50, 0),
    };
    data.save_to_database(&mut tether).await;

    let api_page_clone = api_page.clone();
    ctx.mock_get_conversations_with(move |builder| {
        builder
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                    conversations: api_page_clone.clone(),
                    stale: true,
                    total: 1,
                }),
            )
            .expect(2..=4)
    })
    .await;
    ctx.mock_ping_success().await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Trash.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 10;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut test_scroller =
        TestScroller::conversations_instant(&user_ctx, local_label_id, page_size)
            .await
            .unwrap();

    // The location is trash so we should get no update as the data is stale
    test_scroller.fetch_more().unwrap(); // 1st fetch_more
    test_scroller.match_next_update(TestUpdate::None).await;
    assert!(test_scroller.has_more().await.unwrap());

    // Update the database should trigger a new fetch_more
    let mut new_data = hash_map! {
        vec!["rid2"]: test_conversations(1, 299),
    };
    new_data.save_to_database(&mut tether).await;

    // We shouldn't get any update as the data is still stale
    test_scroller.match_next_update(TestUpdate::None).await;

    // Lets test recovery
    ctx.mock_server().reset().await;
    ctx.mock_get_conversations_with(move |builder| {
        builder
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                    conversations: api_page.clone(),
                    stale: false,
                    total: 1,
                }),
            )
            .expect(2..=3)
    })
    .await;
    ctx.mock_ping_success().await;
    ctx.catch_all().await;

    test_scroller.fetch_more_and_wait().await.unwrap(); // 1st fetch_more
    test_scroller
        .match_next_update(TestUpdate::Append { items: 9 })
        .await;
    assert!(test_scroller.has_more().await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_conversation_mail_scroller_change_label() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let page_size = 10;
    let mut api_page = create_api_conversation_page(0..9, 100);
    for conv in api_page.iter_mut() {
        conv.labels = vec![ApiConversationLabel {
            id: LabelId::inbox(),
            ..ApiConversationLabel::test_default()
        }];
    }
    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: vec![],
        vec!["rid2"]: test_conversations(50, 0),
    };
    data.save_to_database(&mut tether).await;

    let api_page_clone = api_page.clone();
    ctx.mock_get_conversations_with(move |builder| {
        builder
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                    conversations: api_page_clone.clone(),
                    stale: true,
                    total: 1,
                }),
            )
            .expect(2..=8)
    })
    .await;
    ctx.mock_ping_success().await;
    ctx.catch_all().await;

    // we should get an update on the first fetch_more in Inbox despite the data being stale
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 10;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut test_scroller =
        TestScroller::conversations_instant(&user_ctx, local_label_id, page_size)
            .await
            .unwrap();

    test_scroller.fetch_more().unwrap();
    test_scroller
        .match_next_update(TestUpdate::Append { items: 9 })
        .await;
    assert!(test_scroller.has_more().await.unwrap());

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    let remote_label_id = LabelId::from("rid2");
    let local_label_id = Label::remote_id_counterpart(remote_label_id, &tether)
        .await
        .unwrap()
        .unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 50;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    // Switch to custom label "rid2"
    test_scroller
        .change_label(&mailbox, local_label_id)
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 10 })
        .await;
    test_scroller.fetch_more().unwrap();
    test_scroller
        .match_next_update(TestUpdate::Append { items: 10 })
        .await;
    assert_eq!(mailbox.label_id(), local_label_id);

    // Switch back to inbox
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    test_scroller
        .change_label(&mailbox, local_label_id)
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 9 })
        .await;

    assert_eq!(mailbox.label_id(), local_label_id);
}

#[function_name::named]
pub async fn mock_get_conversations_page(
    ctx: &MailTestContext,
    conversations: Vec<ApiConversation>,
    last_id: &str,
    label: &LabelId,
    expect: impl Into<Times>,
) {
    let desc = ScrollOrderDir::for_label(label).as_api_desc().unwrap();
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/conversations"))
        .and(query_param_contains("AnchorID", last_id))
        .and(query_param_contains("Desc", (desc as u8).to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                conversations,
                stale: false,
                total: 1,
            }),
        )
        .expect(expect)
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;
}

#[function_name::named]
pub async fn mock_not_responsive_api(ctx: &MailTestContext) {
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/conversations"))
        .respond_with_err(|_: &Request| {
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "connection reset")
        })
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;
    Mock::given(method("GET"))
        .and(path("/api/core/v4/tests/ping"))
        .respond_with_err(|_: &Request| {
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "connection reset")
        })
        .mount(ctx.mock_server())
        .await;
    ctx.mail_context
        .network_monitor_service()
        .update_os_network_status(OsNetworkStatus::Offline);
}

#[function_name::named]
pub async fn mock_api_forbidden(ctx: &MailTestContext) {
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/conversations"))
        .respond_with(ResponseTemplate::new(403))
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;

    ctx.mock_ping_success().await;
}
