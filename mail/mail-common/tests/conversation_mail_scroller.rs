use itertools::Itertools;
use maplit::hashmap;
use proton_api_core::{service::ApiServiceError, services::proton::common::LabelId};
use proton_api_mail::services::proton::{
    common::ConversationId, prelude::GetConversationsResponse,
    response_data::Conversation as ApiConversation,
};
use proton_core_common::{
    datatypes::SystemLabel,
    models::{Label, ModelExtension, ModelIdExtension},
};
use proton_mail_common::{
    datatypes::{ContextualConversation, ReadFilter},
    mail_scroller::{DataScrollerSource, MailScroller},
    models::{Conversation, ConversationCounters, ConversationScrollData},
    MailContextError,
};
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::{
    conv_id, conv_label, conversation, label, lbl_id, test_context::MailTestContext,
};
use stash::{
    orm::Model,
    stash::{Bond, Tether, WatcherHandle},
};
use std::collections::HashMap;
use wiremock::{
    matchers::{method, path, query_param_contains},
    Mock, ResponseTemplate,
};

fn test_conversations(n: usize, order_shift: u64) -> Vec<Conversation> {
    (0..n)
        .map(|i| {
            let order = i as u64 + order_shift;
            conversation!(remote_id: conv_id!(format!("myconv_{order}")), display_order: order)
        })
        .collect()
}

async fn save_single_conversation(label: &Label, conversation: &mut Conversation, bond: &Bond<'_>) {
    conversation.save(bond).await.unwrap();
    let mut conv_label = conv_label!(
        local_conversation_id: conversation.local_id,
        remote_label_id: label.remote_id.clone(),
        local_label_id: label.local_id,
        context_time: 0
    );

    conv_label.save(bond).await.unwrap();
    conversation.reload(bond).await.unwrap();
}

async fn save_to_database(data: &mut HashMap<&str, Vec<Conversation>>, tether: &mut Tether) {
    let bond = tether.transaction().await.unwrap();

    for (label_rid, conversations) in data.iter_mut() {
        let mut label = label!(remote_id: lbl_id!(label_rid));
        label.save(&bond).await.unwrap();
        let mut counters = ConversationCounters::new(label.local_id.unwrap());
        counters.total = conversations.len() as u64;
        counters.save(&bond).await.unwrap();

        for conversation in conversations.iter_mut() {
            save_single_conversation(&label, conversation, &bond).await;
        }
    }

    bond.commit().await.unwrap()
}

fn expected_conversations(
    n: usize,
    label_id: &str,
    data: &HashMap<&str, Vec<Conversation>>,
) -> Option<Vec<ContextualConversation>> {
    let convs = data.get(label_id)?;
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
async fn test_conversation_mail_scroller_reads_correct_items_within_visible_range_for_cached_scroll_data(
) {
    const REMOTE_LABEL_ID: &str = "rid1";
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();

    let mut data: HashMap<&str, Vec<Conversation>> = hashmap! {
        REMOTE_LABEL_ID => test_conversations(100, 100),
        "rid2" => test_conversations(50, 0),
    };

    save_to_database(&mut data, &mut tether).await;

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
        .build();

    let bond = tether.transaction().await.unwrap();
    scroller.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let page_size = 5;
    let mut scroller = MailScroller::conversations(user_ctx, local_label_id, unread, page_size)
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
    let user_ctx = ctx.mail_user_context().await;
    let tether = user_ctx.user_stash().connection();
    let params = TestParams::default_basic();
    let conversations = params.conversations.clone();
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;

    let page_size = 5;
    let mut scroller = MailScroller::conversations(user_ctx, local_label_id, unread, page_size)
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
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let params = setup_api_conversation_pages(&ctx, page_size, 0, 3).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;

    // Update the inbox label to have all conversations
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = page_size as u64 * 2;
    let bond = tether.transaction().await.unwrap();
    counters.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    // Online
    let mut scroller =
        MailScroller::conversations(user_ctx.clone(), local_label_id, unread, page_size)
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

    // Cached - it will trigger two more background requests for pages as we fetch more
    // This is because cursor have only two pages in cache, which means we will try to get new page evertime we fetch more

    let mut scroller = MailScroller::conversations(user_ctx, local_label_id, unread, page_size)
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
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let unread = ReadFilter::All;
    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let params = setup_api_conversation_pages(&ctx, page_size, 0, 2).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;
    ctx.catch_all().await;

    // Update the inbox label to have all conversations
    let label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = page_size as u64 * 2;
    let bond = tether.transaction().await.unwrap();
    counters.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let mut scroller =
        MailScroller::conversations(user_ctx.clone(), local_label_id, unread, page_size)
            .await
            .unwrap();
    let WatcherHandle {
        handle: _handle,
        receiver,
        ..
    } = scroller.watch().unwrap();
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
    let bond = tether.transaction().await.unwrap();
    save_single_conversation(&label, &mut test_conversation.clone(), &bond).await;
    bond.commit().await.unwrap();
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
async fn test_conversation_mail_scroller_reads_online_folder_for_the_first_time_when_get_an_error_on_request(
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;

    mock_api_forbidden(&ctx).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 1;
    let bond = tether.transaction().await.unwrap();
    counters.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let page_size = 5;
    let mut scroller = MailScroller::conversations(user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items can be read only when we progress with `fetch_more`
    let actual = scroller.fetch_more().await.unwrap_err();
    assert!(matches!(
        actual,
        MailContextError::Api(ApiServiceError::OtherHttpError(..))
    ));
    assert_eq!(
        actual.to_string(),
        "API Error: HTTP error 403 Forbidden: 403 Forbidden. ".to_string()
    );
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);
    assert!(scroller.has_more().await.unwrap());

    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: HTTP error 403 Forbidden: 403 Forbidden. ".to_string()
    );
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_offline_folder_for_the_first_time_and_cache_is_empty(
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;

    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 1;
    let bond = tether.transaction().await.unwrap();
    counters.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let page_size = 5;
    let mut scroller = MailScroller::conversations(user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items can be read only when we progress with `fetch_more`
    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: Network error: No connection".to_string()
    );
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);
    assert!(scroller.has_more().await.unwrap());

    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: Network error: No connection".to_string()
    );
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_offline_folder_for_the_first_time_and_cache_has_one_item(
) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;
    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data: HashMap<&str, Vec<Conversation>> = hashmap! {
        remote_label_id.as_str() => test_conversations(1, 100),
        "rid2" => test_conversations(50, 0),
    };
    save_to_database(&mut data, &mut tether).await;

    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 10;
    let bond = tether.transaction().await.unwrap();
    counters.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let page_size = 5;
    let mut scroller = MailScroller::conversations(user_ctx, local_label_id, unread, page_size)
        .await
        .unwrap();

    // First call is empty
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 0);

    // The items will be read from cache as the API is unreachable
    /*
    let actual = scroller.fetch_more().await.unwrap();
    assert_eq!(actual.len(), 1);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 1);
    assert!(scroller.has_more().await.unwrap());
    */

    // No more cached, no API connection, return error
    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: Network error: No connection".to_string()
    );
}

#[tokio::test]
async fn test_conversation_mail_scroller_reads_cached_data_and_return_error_on_offline_fetch_more()
{
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let unread = ReadFilter::All;

    // Set up cached data
    let remote_label_id = SystemLabel::Inbox.remote_id();
    let mut data: HashMap<&str, Vec<Conversation>> = hashmap! {
        remote_label_id.as_str() => test_conversations(100, 100),
        "rid2" => test_conversations(50, 0),
    };

    save_to_database(&mut data, &mut tether).await;
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
        .build();

    let bond = tether.transaction().await.unwrap();
    scroller.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    // Mock offline
    mock_not_responsive_api(&ctx).await;
    ctx.catch_all().await;

    let mut counters = ConversationCounters::new(local_label_id);
    counters.total = 150;
    let bond = tether.transaction().await.unwrap();
    counters.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let page_size = 50;
    let mut scroller = MailScroller::conversations(user_ctx, local_label_id, unread, page_size)
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
    /*
    let actual = scroller.fetch_more().await.unwrap();

    assert_eq!(actual.len(), 50);
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 100);
    assert!(scroller.has_more().await.unwrap());
    */

    let actual = scroller.fetch_more().await.unwrap_err();
    assert_eq!(
        actual.to_string(),
        "API Error: Network error: No connection".to_string()
    );
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

async fn setup_api_conversation_pages(
    ctx: &MailTestContext,
    page_size: usize,
    starting_display_order: u64,
    empty_pages_requests: u64,
) -> TestParams {
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

    mock_ping_success(ctx).await;
}

pub async fn mock_ping_success(ctx: &MailTestContext) {
    Mock::given(method("GET"))
        .and(path("/api/core/v4/tests/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(ctx.mock_server())
        .await;
}
