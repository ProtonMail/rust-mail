use itertools::Itertools;
use proton_core_common::{
    datatypes::SystemLabel,
    models::{Address, Label, ModelExtension, ModelIdExtension},
};
use proton_mail_api::services::proton::{
    common::MessageId, prelude::GetMessagesResponse,
    response_data::MessageMetadata as ApiMessageMetadata,
};
use proton_mail_common::{
    datatypes::SearchOptions,
    mail_scroller::MailScroller,
    models::{Conversation, Message},
};
use proton_mail_test_utils::api_message_meta;
use proton_mail_test_utils::{init::Params as TestParams, test_context::MailTestContext};
use proton_mail_test_utils::{message, msg_id};

use stash::stash::StashError;
use stash::{
    orm::Model,
    stash::{Bond, WatcherHandle},
};
use std::vec;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path, query_param_contains},
};

async fn save_single_message(label: &Label, message: &mut Message, bond: &Bond<'_>) {
    message.label_ids = vec![label.remote_id.clone().unwrap()];
    message.save(bond).await.unwrap();
    message.reload(bond).await.unwrap();
}

#[tokio::test]
async fn test_search_mail_scroller_reads_one_item_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();
    let message = api_message_meta!(
        id: MessageId::from("mymsg"),
        conversation_id: conversation.id,
        address_id: address.id,
        label_ids: vec![SystemLabel::AllMail.remote_id()]
    );
    ctx.mock_get_messages_total_expect(vec![message], 1, 2)
        .await;
    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    let user_ctx = ctx.mail_user_context().await;

    let page_size = 5;
    let mut scroller =
        MailScroller::search(user_ctx.as_weak(), SearchOptions::default(), page_size)
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
async fn test_search_mail_scroller_reads_two_pages_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let page_size = 5;
    let search_phrase = "Invoice 2024";
    let params = setup_api_message_pages(&ctx, page_size, search_phrase, 2).await;

    ctx.setup_user(params.clone()).await;
    let user_ctx = ctx.mail_user_context().await;

    // Online
    let mut scroller = MailScroller::search(
        user_ctx.as_weak(),
        SearchOptions::from(search_phrase),
        page_size,
    )
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
    assert!(scroller.fetch_more().await.unwrap().is_empty());

    // Search always relay on online data even for the same options used just before.
    let mut scroller = MailScroller::search(
        user_ctx.as_weak(),
        SearchOptions::from(search_phrase),
        page_size,
    )
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
    assert_eq!(scroller.total(), 10);

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
    assert_eq!(scroller.total(), 10);
    assert!(scroller.fetch_more().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_search_mail_scroller_notificate_about_changes() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection();
    let page_size = 5;
    let search_phrase = "123";
    let local_label_id = SystemLabel::AllMail
        .local_id(&tether)
        .await
        .unwrap()
        .unwrap();
    let params = setup_api_message_pages(&ctx, page_size, search_phrase, 1).await;

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let mut scroller = MailScroller::search(
        user_ctx.as_weak(),
        SearchOptions::from(search_phrase),
        page_size,
    )
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

    // Fetching for next, empty page will not trigger any notification

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

    // The new message will not be included in the scroller as it may not match the search criteria
    // It is up to client to run the search again to get the new message
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
}

async fn setup_api_message_pages(
    ctx: &MailTestContext,
    page_size: usize,
    search_phrase: &str,
    expect: u64,
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

    // Messages in search are returned in exact order response provides
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
    let total = (page_size * 2) as u64;

    mock_get_messages_page(
        ctx,
        second_page,
        total,
        search_phrase,
        &first_page_last_id,
        expect,
    )
    .await;
    // last page is empty
    mock_get_messages_page(
        ctx,
        vec![],
        total,
        search_phrase,
        &second_page_last_id,
        expect,
    )
    .await;
    ctx.mock_get_messages_total_expect(first_page, total, expect)
        .await;

    params
}

#[function_name::named]
pub async fn mock_get_messages_page(
    ctx: &MailTestContext,
    messages: Vec<ApiMessageMetadata>,
    total: u64,
    search_phrase: &str,
    last_id: &str,
    expect: u64,
) {
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/messages"))
        .and(query_param_contains("EndID", last_id))
        .and(query_param_contains("Keyword", search_phrase))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(GetMessagesResponse {
                total,
                messages,
                stale: false,
            }),
        )
        .expect(expect)
        .named(function_name!())
        .mount(ctx.mock_server())
        .await;
}
