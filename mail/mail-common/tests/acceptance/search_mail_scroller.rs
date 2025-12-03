use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::SystemLabel;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_common::api_message_meta;
use proton_mail_common::datatypes::{AlmostAllMail, IncludeSwitch, SearchOptions, SystemLabelId};
use proton_mail_common::models::{MailSettings, Message};
use proton_mail_common::msg_id;
use proton_mail_common::test_utils::scroller::{TestScroller, TestUpdate, save_single_message};
use proton_mail_common::test_utils::{init::Params as TestParams, test_context::MailTestContext};
use stash::orm::Model;
use stash::stash::StashError;
use std::time::Duration;
use std::vec;

#[tokio::test]
async fn reads_one_item_from_online_scroll_data() {
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

    ctx.mock_get_messages()
        .given_label_id(&LabelId::almost_all_mail())
        .expect(2)
        .respond_with(vec![message])
        .await;

    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let page_size = 5;

    let mut test_scroller = TestScroller::search(&user_ctx, SearchOptions::default(), page_size)
        .await
        .unwrap();

    assert!(
        test_scroller.supports_include_filter().await,
        "Scroller supports include-filter, because we're looking at the \
         AlmostAllMail label"
    );

    // Search scroller needs explicit fetch_more() call to start fetching data
    test_scroller.fetch_more_and_wait().await.unwrap();
    let actual = test_scroller.items();
    assert_eq!(actual.len(), 1);
    assert_eq!(test_scroller.items().len(), 1);
    assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg"));
    assert!(!test_scroller.has_more().await.unwrap());

    // Additional fetch_more should result in no new data
    let next_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(next_page.is_empty());
}

#[tokio::test]
async fn reads_two_pages_from_online_scroll_data() {
    let ctx = MailTestContext::new().await;
    let page_size = 5;
    let label = SystemLabelId::almost_all_mail();
    let keyword = "Invoice 2024";

    let params = setup_api_message_pages(&ctx, page_size, &label, keyword, 2).await;

    ctx.setup_user(params.clone()).await;
    let user_ctx = ctx.mail_user_context().await;

    // Online
    let mut test_scroller =
        TestScroller::search(&user_ctx, SearchOptions::from(keyword), page_size)
            .await
            .unwrap();

    test_scroller.fetch_more_and_wait().await.unwrap();

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

    // Search always relay on online data even for the same options used just before.
    let mut test_scroller =
        TestScroller::search(&user_ctx, SearchOptions::from(keyword), page_size)
            .await
            .unwrap();

    test_scroller.fetch_more_and_wait().await.unwrap();

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
    assert_eq!(test_scroller.total().await.unwrap(), 10);

    test_scroller.fetch_more_and_wait().await.unwrap();

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
    assert_eq!(test_scroller.total().await.unwrap(), 10);

    // Additional fetch_more should result in no new data
    let next_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(next_page.is_empty());
}

#[tokio::test]
async fn does_not_refresh_on_new_message_in_database() {
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
    let mut new_message = message.clone();
    new_message.id = "new_mymsg".into();

    ctx.mock_get_messages()
        .given_label_id(&LabelId::almost_all_mail())
        .expect(2)
        .respond_with(vec![message])
        .await;

    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let new_message = Message::from_api_metadata(new_message, &tether)
        .await
        .unwrap();

    let page_size = 5;

    let mut test_scroller = TestScroller::search(&user_ctx, SearchOptions::default(), page_size)
        .await
        .unwrap();

    test_scroller.fetch_more_and_wait().await.unwrap();
    let actual = test_scroller.items();
    assert_eq!(actual.len(), 1);
    assert_eq!(test_scroller.items().len(), 1);
    assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg"));
    assert!(!test_scroller.has_more().await.unwrap());

    // Add a new message to the database
    let label = SystemLabel::AllMail.load(&tether).await.unwrap().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            save_single_message(&[label], &mut new_message.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();
    let possible_update = test_scroller
        .try_wait_for_update(Duration::from_secs(1))
        .await
        .unwrap();

    // Search scroller does not refresh on new message in database
    assert!(possible_update.is_none());
    assert_eq!(test_scroller.items().len(), 1);
    // Request refresh to ensure we won't get any updates
    let actual = test_scroller.refresh_and_wait().await.unwrap();
    assert!(actual.is_empty());

    // Additional fetch_more should result in no new data
    let next_page = test_scroller.fetch_more_and_wait().await.unwrap();
    assert!(next_page.is_empty());
}

#[tokio::test]
async fn does_refresh_on_modified_message_in_database() {
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
    let mut new_message = message.clone();
    new_message.unread = true;

    ctx.mock_get_messages()
        .given_label_id(&LabelId::almost_all_mail())
        .expect(2)
        .respond_with(vec![message])
        .await;

    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let new_message = Message::from_api_metadata(new_message, &tether)
        .await
        .unwrap();

    let page_size = 5;

    let mut test_scroller = TestScroller::search(&user_ctx, SearchOptions::default(), page_size)
        .await
        .unwrap();

    test_scroller.fetch_more_and_wait().await.unwrap();
    let actual = test_scroller.items();
    assert_eq!(actual.len(), 1);
    assert_eq!(test_scroller.items().len(), 1);
    assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg"));
    assert!(!test_scroller.has_more().await.unwrap());

    // Add a new message to the database
    let label = SystemLabel::AllMail.load(&tether).await.unwrap().unwrap();
    tether
        .tx::<_, _, StashError>(async |bond| {
            save_single_message(&[label], &mut new_message.clone(), bond).await;
            Ok(())
        })
        .await
        .unwrap();
    let possible_update = test_scroller
        .try_wait_for_update(Duration::from_secs(1))
        .await
        .unwrap();

    // Search scroller will refresh on modified message in database which is included in the search
    assert!(possible_update.is_some());
    assert_eq!(test_scroller.items().len(), 1);
    assert!(test_scroller.items()[0].unread);
}

#[tokio::test]
async fn all_mail() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();

    // ---

    let message = api_message_meta!(
        id: MessageId::from("mymsg"),
        conversation_id: conversation.id,
        address_id: address.id,
        label_ids: vec![SystemLabel::AllMail.remote_id()]
    );

    ctx.mock_get_messages()
        .given_label_id(&LabelId::all_mail())
        .expect(1..=2)
        .respond_with(vec![message])
        .await;

    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;

    // ---

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let mut settings = MailSettings::get_or_default(&tether).await;

    settings.almost_all_mail = AlmostAllMail::AllMail;

    tether
        .tx(async |bond| settings.save(bond).await)
        .await
        .unwrap();

    // ---

    let page_size = 5;

    let mut test_scroller = TestScroller::search(&user_ctx, SearchOptions::default(), page_size)
        .await
        .unwrap();

    assert!(
        !test_scroller.supports_include_filter().await,
        "Scroller doesn't support include-filter, because we're already \
         looking at the AllMail label"
    );

    test_scroller.fetch_more_and_wait().await.unwrap();

    let actual = test_scroller.items();

    assert_eq!(actual.len(), 1);
    assert_eq!(test_scroller.items().len(), 1);
    assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg"));
}

#[tokio::test]
async fn almost_all_mail_with_spam_and_trash() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();

    // ---

    let message1 = api_message_meta!(
        id: MessageId::from("mymsg1"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::AllMail.remote_id()]
    );

    let message2 = api_message_meta!(
        id: MessageId::from("mymsg2"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::Spam.remote_id()]
    );

    ctx.mock_get_messages()
        .given_label_id(&LabelId::almost_all_mail())
        .expect(1..=2)
        .respond_with(vec![message1.clone()])
        .await;

    ctx.mock_get_messages()
        .given_label_id(&LabelId::all_mail())
        .given_keyword("keyword")
        .expect(1..=2)
        .respond_with(vec![message2.clone()])
        .await;

    ctx.mock_get_messages()
        .given_label_id(&LabelId::all_mail())
        .expect(1..=2)
        .respond_with(vec![message1, message2])
        .await;

    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;

    // ---

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let mut settings = MailSettings::get_or_default(&tether).await;

    settings.almost_all_mail = AlmostAllMail::AlmostAllMail;

    tether
        .tx(async |bond| settings.save(bond).await)
        .await
        .unwrap();

    // ---

    let page_size = 5;

    let mut test_scroller = TestScroller::search(&user_ctx, SearchOptions::default(), page_size)
        .await
        .unwrap();

    assert!(
        test_scroller.supports_include_filter().await,
        "Scroller supports include-filter, because originally we're looking at \
         the AlmostAllMail label"
    );

    test_scroller.fetch_more_and_wait().await.unwrap();
    {
        let actual = test_scroller.items();

        assert_eq!(actual.len(), 1);
        assert_eq!(test_scroller.items().len(), 1);
        assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg1"));
    }
    test_scroller
        .change_include(IncludeSwitch::WithSpamAndTrash)
        .unwrap();

    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 2 })
        .await;
    {
        let actual = test_scroller.items();
        assert_eq!(actual.len(), 2);
        assert_eq!(test_scroller.items().len(), 2);
        assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg1"));
        assert_eq!(actual[1].remote_id.clone(), msg_id!("mymsg2"));
    }

    test_scroller
        .change_keywords(SearchOptions::from("keyword"))
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 1 })
        .await;

    let actual = test_scroller.items();
    assert_eq!(actual.len(), 1);
    assert_eq!(test_scroller.items().len(), 1);
    assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg2"));
}

#[tokio::test]
async fn change_include_multiple_times_in_a_row() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();

    // ---

    let message1 = api_message_meta!(
        id: MessageId::from("mymsg1"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::AllMail.remote_id()]
    );

    let message2 = api_message_meta!(
        id: MessageId::from("mymsg2"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::Spam.remote_id()]
    );

    ctx.mock_get_messages()
        .given_label_id(&LabelId::almost_all_mail())
        .given_end_id(message1.id.as_str())
        .expect(2..=4)
        .respond_with(vec![])
        .await;

    ctx.mock_get_messages()
        .given_label_id(&LabelId::almost_all_mail())
        .expect(2..=4)
        .respond_with(vec![message1.clone()])
        .await;

    ctx.mock_get_messages()
        .given_label_id(&LabelId::all_mail())
        .given_end_id(message2.id.as_str())
        .expect(2..=4)
        .respond_with(vec![])
        .await;

    ctx.mock_get_messages()
        .given_label_id(&LabelId::all_mail())
        .expect(2..=4)
        .respond_with(vec![message1, message2])
        .await;

    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;

    // ---

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let mut settings = MailSettings::get_or_default(&tether).await;

    settings.almost_all_mail = AlmostAllMail::AlmostAllMail;

    tether
        .tx(async |bond| settings.save(bond).await)
        .await
        .unwrap();

    // ---

    let page_size = 5;

    let mut test_scroller = TestScroller::search(&user_ctx, SearchOptions::default(), page_size)
        .await
        .unwrap();

    assert!(
        test_scroller.supports_include_filter().await,
        "Scroller supports include-filter, because originally we're looking at \
         the AlmostAllMail label"
    );

    test_scroller.fetch_more_and_wait().await.unwrap();
    {
        let actual = test_scroller.items();

        assert_eq!(actual.len(), 1);
        assert_eq!(test_scroller.items().len(), 1);
        assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg1"));
    }

    test_scroller
        .change_include(IncludeSwitch::WithSpamAndTrash)
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 2 })
        .await;
    test_scroller
        .change_include(IncludeSwitch::Default)
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 1 })
        .await;
    test_scroller
        .change_include(IncludeSwitch::WithSpamAndTrash)
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 2 })
        .await;
    test_scroller
        .change_include(IncludeSwitch::Default)
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 1 })
        .await;
    test_scroller
        .change_include(IncludeSwitch::WithSpamAndTrash)
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 2 })
        .await;
    test_scroller
        .change_include(IncludeSwitch::Default)
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 1 })
        .await;
}

#[tokio::test]
async fn change_keywords_multiple_times_in_a_row() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let conversation = params.conversations.first().cloned().unwrap();
    let address = params.addresses.first().cloned().unwrap();

    // ---

    let message1 = api_message_meta!(
        id: MessageId::from("mymsg1"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::AllMail.remote_id()]
    );

    let message2 = api_message_meta!(
        id: MessageId::from("mymsg2"),
        conversation_id: conversation.id.clone(),
        address_id: address.id.clone(),
        label_ids: vec![SystemLabel::Spam.remote_id()]
    );

    ctx.mock_get_messages()
        .given_label_id(&LabelId::almost_all_mail())
        .given_keyword("keyword")
        .expect(6..=8)
        .respond_with(vec![message1.clone()])
        .await;

    ctx.mock_get_messages()
        .given_label_id(&LabelId::almost_all_mail())
        .given_keyword("other keyword")
        .expect(6..=8)
        .respond_with(vec![message1, message2])
        .await;

    ctx.mock_ping_success().await;
    ctx.setup_user(params.clone()).await;

    // ---

    let user_ctx = ctx.mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let mut settings = MailSettings::get_or_default(&tether).await;

    settings.almost_all_mail = AlmostAllMail::AlmostAllMail;

    tether
        .tx(async |bond| settings.save(bond).await)
        .await
        .unwrap();

    // ---

    let page_size = 5;
    let keywords = SearchOptions::from("keyword");
    let other_keywords = SearchOptions::from("other keyword");

    let mut test_scroller = TestScroller::search(&user_ctx, keywords.clone(), page_size)
        .await
        .unwrap();

    assert!(
        test_scroller.supports_include_filter().await,
        "Scroller supports include-filter, because originally we're looking at \
         the AlmostAllMail label"
    );

    test_scroller.fetch_more_and_wait().await.unwrap();
    {
        let actual = test_scroller.items();

        assert_eq!(actual.len(), 1);
        assert_eq!(test_scroller.items().len(), 1);
        assert_eq!(actual[0].remote_id.clone(), msg_id!("mymsg1"));
    }

    test_scroller
        .change_keywords(other_keywords.clone())
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 2 })
        .await;
    test_scroller.change_keywords(keywords.clone()).unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 1 })
        .await;
    test_scroller
        .change_keywords(other_keywords.clone())
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 2 })
        .await;
    test_scroller.change_keywords(keywords.clone()).unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 1 })
        .await;
    test_scroller
        .change_keywords(other_keywords.clone())
        .unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 2 })
        .await;
    test_scroller.change_keywords(keywords.clone()).unwrap();
    test_scroller
        .match_next_update(TestUpdate::ReplaceFrom { idx: 0, items: 1 })
        .await;
}

async fn setup_api_message_pages(
    ctx: &MailTestContext,
    page_size: usize,
    label_id: &LabelId,
    keyword: &str,
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
    let total = page_size * 2;

    ctx.mock_get_messages()
        .given_label_id(label_id)
        .given_keyword(keyword)
        .given_end_id(&first_page_last_id)
        .expect(expect)
        .respond_with_ex(total, second_page)
        .await;

    ctx.mock_get_messages()
        .given_label_id(label_id)
        .given_keyword(keyword)
        .given_end_id(&second_page_last_id)
        .expect(expect)
        .respond_with_ex(total, Vec::new())
        .await;

    ctx.mock_get_messages()
        .expect(expect)
        .respond_with_ex(total, first_page)
        .await;

    params
}
