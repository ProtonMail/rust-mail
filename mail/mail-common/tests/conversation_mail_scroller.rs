use itertools::Itertools;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::LabelId;
use proton_core_common::{
    datatypes::SystemLabel,
    models::{Label, ModelIdExtension},
};
use proton_mail_api::services::proton::{
    common::ConversationId, prelude::GetConversationsResponse,
    response_data::Conversation as ApiConversation,
};
use proton_mail_common::datatypes::labels::LabelScrollOrder;
use proton_mail_common::test_utils::{
    init::Params as TestParams,
    scroller::{StoreLabeledModelMap, save_single_conversation, test_conversations},
    test_context::MailUserContextTestExtension,
};
use proton_mail_common::{
    MailContextError,
    datatypes::{ContextualConversation, ReadFilter},
    mail_scroller::{DataScrollerSource, MailScroller},
    models::{Conversation, ConversationCounters, ConversationScrollData},
};
use proton_mail_common::{conv_id, lbl_id, test_utils::test_context::MailTestContext};
use stash::stash::StashError;
use stash::{orm::Model, stash::WatcherHandle};
use std::{collections::HashMap, time::Duration};
use velcro::hash_map;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path, query_param_contains},
};

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
    let mut tether = user_ctx.user_stash().connection();

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
        .display_order(last_conversation.display_order)
        .scroll_order(LabelScrollOrder::Descending)
        .build();

    tether
        .tx(async |bond| scroller.save(bond).await)
        .await
        .unwrap();

    let page_size = 5;
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();
    let expected = expected_conversations(page_size, REMOTE_LABEL_ID, &data).unwrap();
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual, expected);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual, expected);

    assert!(scroller.has_more().await.unwrap());

    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), page_size);

    let actual = scroller.all_items().await.unwrap();
    let expected = expected_conversations(page_size * 2, REMOTE_LABEL_ID, &data).unwrap();

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_one_item_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversations = params.conversations.clone();

    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;

    let page_size = 5;
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items can be read only when we progress with `fetch_more`
    let expected = scroller.fetch_more().await.unwrap();
    let mut actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(actual, expected);
    let actual = actual.pop().unwrap();
    assert_eq!(actual.remote_id, conv_id!("myconv"));
    assert!(!scroller.has_more().await.unwrap());

    let next_page = scroller.fetch_more().await.unwrap();
    assert_eq!(next_page.len(), 0);
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_two_pages_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    setup_api_sync_previous_page(&ctx, "myconv_9", 1).await;
    let params = setup_api_conversation_pages(&ctx, page_size, 0, 3).await;
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
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();
    // Conversations can be accessed only when progressed.
    scroller.fetch_more().await.unwrap();
    assert_scroller_content(
        &mut scroller,
        5,
        &["myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5"],
    )
    .await;
    assert!(scroller.has_more().await.unwrap());

    // Get next page - it will progress cursor to the next page
    // But there is no more data available, the request will return an empty page
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 5);
    assert_scroller_content(
        &mut scroller,
        10,
        &[
            "myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5", "myconv_4", "myconv_3",
            "myconv_2", "myconv_1", "myconv_0",
        ],
    )
    .await;
    assert!(!scroller.has_more().await.unwrap());

    // Cached - it will trigger two more next page requests for pages as we fetch more
    // and one previous page request on init.
    // This is because cursor have only two pages in cache, which means we will try to get new page evertime we fetch more

    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();
    scroller.fetch_more().await.unwrap();
    assert_scroller_content(
        &mut scroller,
        5,
        &["myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5"],
    )
    .await;
    assert!(scroller.has_more().await.unwrap());

    scroller.fetch_more().await.unwrap();
    assert_scroller_content(
        &mut scroller,
        10,
        &[
            "myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5", "myconv_4", "myconv_3",
            "myconv_2", "myconv_1", "myconv_0",
        ],
    )
    .await;
    assert!(!scroller.has_more().await.unwrap());
}

#[tokio::test]
async fn test_conversation_mail_scroller_notificate_about_changes() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let params = setup_api_conversation_pages(&ctx, page_size, 0, 2).await;

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Update the inbox label to have all conversations
    let label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = page_size as u64 * 2;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
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
    assert_scroller_content(
        &mut scroller,
        5,
        &["myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5"],
    )
    .await;
    // Fetching more will never trigger any notifications
    assert!(receiver.is_empty());

    // Get next page
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 5);
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 0);
    // Fetching more will never trigger any notifications
    assert!(receiver.is_empty());

    assert_scroller_content(
        &mut scroller,
        10,
        &[
            "myconv_9", "myconv_8", "myconv_7", "myconv_6", "myconv_5", "myconv_4", "myconv_3",
            "myconv_2", "myconv_1", "myconv_0",
        ],
    )
    .await;

    // Lets create a new conversation and check if it is added to the scroller
    let test_conversation = test_conversations(1, 100).pop().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            save_single_conversation(&[label], &mut test_conversation.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();
    // Getting an update will trigger a notification
    receiver.recv_async().await.unwrap();
    assert_scroller_content(
        &mut scroller,
        11,
        &[
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
        ],
    )
    .await;
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_online_folder_for_the_first_time_when_get_an_error_on_request()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;

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
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items can be read only when we progress with `fetch_more`
    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: HTTP error 403 Forbidden: 403 Forbidden. None".to_string()
    );

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);
    assert!(scroller.has_more().await.unwrap());

    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: HTTP error 403 Forbidden: 403 Forbidden. None".to_string()
    );
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_offline_folder_for_the_first_time_and_cache_is_empty()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;

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
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items can be read only when we progress with `fetch_more`
    let actual = scroller.fetch_more().await.unwrap_err();
    assert!(
        matches!(
            actual,
            MailContextError::Api(ApiServiceError::NetworkError(_))
        ) || matches!(
            actual,
            MailContextError::Api(ApiServiceError::InternalServerError(_, _))
        )
    );

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);
    assert!(scroller.has_more().await.unwrap());

    let actual = scroller.fetch_more().await.unwrap_err();

    assert!(
        matches!(
            actual,
            MailContextError::Api(ApiServiceError::NetworkError(_))
        ) || matches!(
            actual,
            MailContextError::Api(ApiServiceError::InternalServerError(_, _))
        )
    );
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_offline_folder_for_the_first_time_and_cache_has_one_item()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;
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
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items will be read from cache as the API is unreachable
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 1);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 1);
    assert!(scroller.has_more().await.unwrap());

    // No more cached, no API connection, return error
    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: Network error: No connection".to_string()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn test_conversation_mail_scroller_reads_offline_folder_for_the_first_time_and_cache_has_multiple_pages()
 {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;
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
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items will be read from cache as the API is unreachable
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 5);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 5);
    assert!(scroller.has_more().await.unwrap());
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 6);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 11);

    // No more cached, no API connection, return error
    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: Network error: No connection".to_string()
    );

    // Go online suddenly
    ctx.mock_server().reset().await;
    ctx.mock_ping_success().await;
    setup_api_conversation_pages(&ctx, page_size, 200, 1).await;

    let WatcherHandle {
        receiver,
        handle: _handle,
        ..
    } = scroller.watch().await.unwrap();

    let timeout = Some(Duration::from_secs(3));
    user_ctx
        .wait_for(timeout, |status| status.is_online())
        .await;

    // `all_items` will react to the online data being available
    // listing will be done in correct the order of the cache
    // note: asserting here leads to races between request, db transaction, and the read instant.
    // But `fetch_more` will force replacing unordered items with correct order from API
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 0);

    receiver.recv_timeout(Duration::from_millis(1000)).unwrap();

    assert_scroller_content(
        &mut scroller,
        5,
        &[
            "myconv_209",
            "myconv_208",
            "myconv_207",
            "myconv_206",
            "myconv_205",
        ],
    )
    .await;
    // progress to the next page from API
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 5);

    assert_scroller_content(
        &mut scroller,
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
        ],
    )
    .await;

    // There is no more data in API
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The unordered items are not included in the api response
    // they will not be shown untill we go offline again
    // this is test specific behavior, in real app we should not have such a situation
    // though simillar case is tested here where we do have big location and not all items were fetched
    // during online period
    ctx.mock_server().reset().await;
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;
    user_ctx
        .wait_for(timeout, |status| status.is_offline())
        .await;

    assert_scroller_content(
        &mut scroller,
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
        ],
    )
    .await;

    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 5);

    assert_scroller_content(
        &mut scroller,
        15,
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
            "myconv_110",
            "myconv_109",
            "myconv_108",
            "myconv_107",
            "myconv_106",
        ],
    )
    .await;

    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 6);

    assert_scroller_content(
        &mut scroller,
        21,
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
            "myconv_110",
            "myconv_109",
            "myconv_108",
            "myconv_107",
            "myconv_106",
            "myconv_105",
            "myconv_104",
            "myconv_103",
            "myconv_102",
            "myconv_101",
            "myconv_100",
        ],
    )
    .await;

    // No more items in the cache and we are offline but we satisfied the counter
    // Return empty page instead of the Network error
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 0);
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_cached_data_and_return_error_on_offline_fetch_more()
{
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;

    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(100, 100),
        vec!["rid2"]: test_conversations(50, 0),
    };

    data.save_to_database(&mut tether).await;
    let last_conversation =
        Conversation::find_by_remote_id(ConversationId::from("myconv_150"), &tether)
            .await
            .unwrap()
            .unwrap();
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let last_label = last_conversation.label(local_label_id).unwrap();
    let mut scroller = ConversationScrollData::builder()
        .local_label_id(local_label_id)
        .unread(unread)
        .remote_conversation_id(last_conversation.remote_id.clone().unwrap())
        .conversation_time(last_label.context_time)
        .display_order(last_conversation.display_order)
        .scroll_order(LabelScrollOrder::Descending)
        .build();

    tether
        .tx(async |bond| scroller.save(bond).await)
        .await
        .unwrap();

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
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items can be read only when we progress with `fetch_more`
    let actual = scroller.fetch_more().await.unwrap();

    assert_eq!(actual.len(), 50);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 50);
    assert!(scroller.has_more().await.unwrap());

    // We reached api cached mark, lets serve the rest from cache even if unordered
    let actual = scroller.fetch_more().await.unwrap();

    assert_eq!(actual.len(), 50);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 100);
    assert!(scroller.has_more().await.unwrap());

    // No more cached, no API connection, return error
    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: Network error: No connection".to_string()
    );
}

#[tokio::test]
async fn test_conversation_mail_scroller_has_insufficient_cached_data_to_fill_first_page() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(3, 100),
    };
    data.save_to_database(&mut tether).await;

    setup_api_sync_previous_page(&ctx, "myconv_102", 1).await;
    let params = setup_api_conversation_pages(&ctx, page_size, 0, 2).await;
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
        .conversation_time(last_label.context_time)
        .display_order(last_conversation.display_order)
        .scroll_order(LabelScrollOrder::Descending)
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
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    // Fetch more will load 8 items, 3 + 5 as in total it is less than
    // 2 separate pages so it will merge them together.
    let fetched_page = scroller.fetch_more().await.unwrap();
    assert_eq!(fetched_page.len(), 8);

    assert_scroller_content(
        &mut scroller,
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
        ],
    )
    .await;
    assert!(scroller.has_more().await.unwrap());

    // Get next page - it will progress cursor to the next page
    // Since we started moving by whole pages it will fetch 5 items now
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 5);
    assert_scroller_content(
        &mut scroller,
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
        ],
    )
    .await;
    assert!(!scroller.has_more().await.unwrap());

    // Lets try read it again from cache
    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 5);
    assert_scroller_content(
        &mut scroller,
        5,
        &[
            "myconv_102",
            "myconv_101",
            "myconv_100",
            "myconv_9",
            "myconv_8",
        ],
    )
    .await;
    assert!(scroller.has_more().await.unwrap());

    // This `fetch_more` will join two last pages together as the last page is incomplete
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 8);

    assert_scroller_content(
        &mut scroller,
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
        ],
    )
    .await;
    assert!(!scroller.has_more().await.unwrap());
}

#[tokio::test]
async fn test_conversation_mail_scroller_invalidates_when_dirty() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();

    // Set up cached data with multiple conversations
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data = hash_map! {
        vec![remote_label_id.as_str()]: test_conversations(10, 100),
    };
    data.save_to_database(&mut tether).await;

    // Mock offline to use cached data
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    user_ctx
        .wait_for(Some(Duration::from_secs(5)), |status| status.is_offline())
        .await;

    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 10;
    tether
        .tx(async |bond| counters.save(bond).await)
        .await
        .unwrap();

    let mut scroller =
        MailScroller::conversations(user_ctx.as_weak(), local_label_id, unread, page_size)
            .await
            .unwrap();

    // Set up watcher to detect database changes
    let WatcherHandle {
        handle: _handle,
        receiver,
        ..
    } = scroller.watch().await.unwrap();

    // Initial fetch should work normally
    let first_page = scroller.fetch_more().await.unwrap();
    assert_eq!(first_page.len(), 5);
    assert_eq!(scroller.seen().await.unwrap(), 5);

    // Simulate database change by adding a new conversation
    // This should trigger the watcher and mark the scroller as dirty
    let label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let new_conversation = test_conversations(1, 200).pop().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            save_single_conversation(&[label], &mut new_conversation.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();

    // Wait for the watcher notification which marks the scroller as dirty
    receiver.recv_async().await.unwrap();

    // The critical test: fetch_more should return empty vec when dirty
    // This prevents the race condition where clients might append duplicates
    let dirty_fetch_result = scroller.fetch_more().await.unwrap();
    assert_eq!(
        dirty_fetch_result.len(),
        0,
        "fetch_more should return empty vec when dirty to prevent duplicates"
    );

    // After the dirty invalidation, all_items should show the updated state
    let all_items_after_dirty = scroller.all_items().await.unwrap();
    assert_eq!(all_items_after_dirty.len(), 6); // 5 original + 1 new

    // Verify the new conversation is included and properly ordered
    let conversation_ids: Vec<_> = all_items_after_dirty
        .iter()
        .filter_map(|conv| conv.remote_id.as_ref().map(|id| id.as_str()))
        .collect();
    assert!(conversation_ids.contains(&"myconv_200"));

    // Subsequent fetch_more should work normally again (no longer dirty)
    let next_page = scroller.fetch_more().await.unwrap();
    assert_eq!(next_page.len(), 5); // Rest of the cached conversations
}

async fn assert_scroller_content(
    scroller: &mut MailScroller<DataScrollerSource<ConversationScrollData>>,
    len: usize,
    expected: &'static [&'static str],
) {
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), len);

    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();

    let expected_rids = expected.iter().map(|rid| conv_id!(*rid)).collect_vec();

    assert_eq!(actual_rids, expected_rids);
}

#[function_name::named]
async fn setup_api_sync_previous_page(ctx: &MailTestContext, first_id: &str, expect: u64) {
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/conversations"))
        .and(query_param_contains("BeginID", first_id))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetConversationsResponse {
                conversations: vec![],
                stale: false,
                total: 0,
            }),
        )
        .expect(expect)
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;
}

async fn setup_api_conversation_pages(
    ctx: &MailTestContext,
    page_size: usize,
    starting_display_order: u64,
    empty_pages_requests: u64,
) -> TestParams {
    ctx.mock_ping_success().await;
    let mut params = TestParams::default_basic();
    let test_conversation = params.conversations.clone().pop().unwrap();
    // Conversations are returned and displayed in reversed order
    let second_page = (0..page_size)
        .rev()
        .map(|i| {
            let order = starting_display_order + i as u64;
            let mut new = test_conversation.clone();
            new.id = format!("{}_{}", new.id, order).into();
            new.order = order;
            new.context_time = Some(order);
            new
        })
        .collect_vec();
    let first_page = (page_size..(page_size * 2))
        .rev()
        .map(|i| {
            let order = starting_display_order + i as u64;
            let mut new = test_conversation.clone();
            new.id = format!("{}_{}", new.id, order).into();
            new.order = order;
            new.context_time = Some(order);
            new
        })
        .collect_vec();
    let first_page_last_id = first_page.last().map(|conv| conv.id.to_string()).unwrap();
    let second_page_last_id = second_page.last().map(|conv| conv.id.to_string()).unwrap();

    mock_get_conversations_page(ctx, second_page, &first_page_last_id, 1_u64).await;
    // last page is empty
    mock_get_conversations_page(ctx, vec![], &second_page_last_id, empty_pages_requests).await;
    ctx.mock_get_conversations(first_page, 1_u64).await;

    // Do not download any conv on init
    params.conversations = vec![];
    params
}

#[function_name::named]
pub async fn mock_get_conversations_page(
    ctx: &MailTestContext,
    conversations: Vec<ApiConversation>,
    last_id: &str,
    expect: u64,
) {
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/conversations"))
        .and(query_param_contains("EndID", last_id))
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
        .respond_with(ResponseTemplate::new(500))
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;
    Mock::given(method("GET"))
        .and(path("/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(500))
        .mount(ctx.mock_server())
        .await;
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
