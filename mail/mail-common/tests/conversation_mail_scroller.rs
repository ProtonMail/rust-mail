use itertools::Itertools;
use maplit::hashmap;
use proton_api_core::services::proton::common::LabelId;
use proton_api_mail::services::proton::{
    common::ConversationId, prelude::GetConversationsResponse,
    response_data::Conversation as ApiConversation,
};
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_common::{
    datatypes::{ContextualConversation, ReadFilter, SystemLabel},
    mail_scroller::{MailConversationScrollerSource, MailScroller},
    models::{Conversation, ConversationScrollData, Label},
};
use proton_mail_test_utils::init::Params as TestParams;
use proton_mail_test_utils::{
    conv_id, conv_label, conversation, label, lbl_id, test_context::MailTestContext,
};
use stash::{
    orm::Model,
    stash::{Bond, Tether, WatcherHandle},
};
use std::{collections::HashMap, vec};
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
        context_time: conversation.display_order
    );

    conv_label.save(bond).await.unwrap();
    conversation.reload(bond).await.unwrap();
}

async fn save_to_database(data: &mut HashMap<&str, Vec<Conversation>>, tether: &mut Tether) {
    let bond = tether.transaction().await.unwrap();

    for (label_rid, conversations) in data.iter_mut() {
        let mut label =
            label!(remote_id: lbl_id!(label_rid), total_conv: conversations.len() as u64);
        label.save(&bond).await.unwrap();
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
    let source = MailConversationScrollerSource::new(local_label_id, unread, page_size);
    let mut scroller = MailScroller::new(user_ctx, source).await.unwrap();
    let actual = scroller.all_items().await.unwrap();
    let expected = expected_conversations(page_size, REMOTE_LABEL_ID, &data).unwrap();

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

    // Scroller will ask for two pages, in reality second one should be empty
    // But lets make it trickier and return the same page for the second request
    ctx.mock_get_conversations(conversations, 2_u64).await;
    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;
    ctx.catch_all().await;

    let local_label_id = SystemLabel::Inbox.local_id(&tether).await.unwrap().unwrap();
    let unread = ReadFilter::All;

    let page_size = 5;
    let source = MailConversationScrollerSource::new(local_label_id, unread, page_size);
    let mut scroller = MailScroller::new(user_ctx, source).await.unwrap();

    let mut actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 1);
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
    let params = setup_api_conversation_pages(&ctx, page_size, 3).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;

    // Update the inbox label to have all conversations
    let mut label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    label.total_conv = page_size as u64 * 2;
    let bond = tether.transaction().await.unwrap();
    label.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    // Online
    let source = MailConversationScrollerSource::new(local_label_id, unread, page_size);
    let mut scroller = MailScroller::new(user_ctx.clone(), source).await.unwrap();

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 5);

    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            conv_id!("myconv_9"),
            conv_id!("myconv_8"),
            conv_id!("myconv_7"),
            conv_id!("myconv_6"),
            conv_id!("myconv_5"),
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
            conv_id!("myconv_9"),
            conv_id!("myconv_8"),
            conv_id!("myconv_7"),
            conv_id!("myconv_6"),
            conv_id!("myconv_5"),
            conv_id!("myconv_4"),
            conv_id!("myconv_3"),
            conv_id!("myconv_2"),
            conv_id!("myconv_1"),
            conv_id!("myconv_0"),
        ]
    );
    assert!(!scroller.has_more().await.unwrap());

    // Cached - it will trigger one more background requests for pages as we fetch more
    // This is because cursor have only two pages in cache, which means we will try to get new page while progressing
    // to the second page.

    let source = MailConversationScrollerSource::new(local_label_id, unread, page_size);
    let mut scroller = MailScroller::new(user_ctx, source).await.unwrap();

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 5);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            conv_id!("myconv_9"),
            conv_id!("myconv_8"),
            conv_id!("myconv_7"),
            conv_id!("myconv_6"),
            conv_id!("myconv_5"),
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
            conv_id!("myconv_9"),
            conv_id!("myconv_8"),
            conv_id!("myconv_7"),
            conv_id!("myconv_6"),
            conv_id!("myconv_5"),
            conv_id!("myconv_4"),
            conv_id!("myconv_3"),
            conv_id!("myconv_2"),
            conv_id!("myconv_1"),
            conv_id!("myconv_0"),
        ]
    );
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
    let params = setup_api_conversation_pages(&ctx, page_size, 1).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.setup_user(params.clone()).await;
    ctx.init_user(user_ctx.clone()).await;
    ctx.catch_all().await;

    // Update the inbox label to have all conversations
    let mut label = Label::load(local_label_id, &tether).await.unwrap().unwrap();
    label.total_conv = page_size as u64 * 2;
    let bond = tether.transaction().await.unwrap();
    label.save(&bond).await.unwrap();
    bond.commit().await.unwrap();

    let source = MailConversationScrollerSource::new(local_label_id, unread, page_size);
    let mut scroller = MailScroller::new(user_ctx.clone(), source).await.unwrap();
    let WatcherHandle {
        handle: _handle,
        receiver,
        ..
    } = scroller.watch().unwrap();
    // At this point we have a scroller with one page loaded and one which may be yet loading.
    // There is a case in which there might be a race and notification will be sent before the second page is loaded.
    // This does not hurt anyone but we cannot be sure that we will receive the notification here.

    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 5);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            conv_id!("myconv_9"),
            conv_id!("myconv_8"),
            conv_id!("myconv_7"),
            conv_id!("myconv_6"),
            conv_id!("myconv_5"),
        ]
    );

    // Get next page
    let actual_page = scroller.fetch_more().await.unwrap();
    assert_eq!(actual_page.len(), 5);

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
            conv_id!("myconv_9"),
            conv_id!("myconv_8"),
            conv_id!("myconv_7"),
            conv_id!("myconv_6"),
            conv_id!("myconv_5"),
            conv_id!("myconv_4"),
            conv_id!("myconv_3"),
            conv_id!("myconv_2"),
            conv_id!("myconv_1"),
            conv_id!("myconv_0"),
        ]
    );

    // Lets create a new conversation and check if it is added to the scroller
    let test_conversation = test_conversations(1, 100).pop().unwrap();
    let bond = tether.transaction().await.unwrap();
    save_single_conversation(&label, &mut test_conversation.clone(), &bond).await;
    bond.commit().await.unwrap();
    // Getting an update will trigger a notification
    if receiver.is_empty() {
        receiver.recv_async().await.unwrap();
    } else {
        // We managed to get a notification for the second page request
        receiver.recv_async().await.unwrap();
        receiver.recv_async().await.unwrap();
    }
    let actual = scroller.all_items().await.unwrap();
    assert_eq!(actual.len(), 11);
    let actual_rids = actual
        .iter()
        .map(|conv| conv.remote_id.clone())
        .collect_vec();
    assert_eq!(
        actual_rids,
        vec![
            conv_id!("myconv_100"),
            conv_id!("myconv_9"),
            conv_id!("myconv_8"),
            conv_id!("myconv_7"),
            conv_id!("myconv_6"),
            conv_id!("myconv_5"),
            conv_id!("myconv_4"),
            conv_id!("myconv_3"),
            conv_id!("myconv_2"),
            conv_id!("myconv_1"),
            conv_id!("myconv_0"),
        ]
    );
}

async fn setup_api_conversation_pages(
    ctx: &MailTestContext,
    page_size: usize,
    empty_pages_requests: u64,
) -> TestParams {
    let mut params = TestParams::default_basic();
    let test_conversation = params.conversations.clone().pop().unwrap();
    // Conversations are returned and displayed in reversed order
    let second_page = (0..page_size)
        .rev()
        .map(|i| {
            let mut new = test_conversation.clone();
            new.id = format!("{}_{}", new.id, i).into();
            new.order = i as u64;
            new
        })
        .collect_vec();
    let first_page = (page_size..(page_size * 2))
        .rev()
        .map(|i| {
            let mut new = test_conversation.clone();
            new.id = format!("{}_{}", new.id, i).into();
            new.order = i as u64;
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
        .mount(ctx.mock_server())
        .await;
}
