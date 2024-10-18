use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::requests::{GetConversationsOptions, GetMessagesOptions};
use proton_core_common::{
    datatypes::RemoteId,
    models::{Address, ModelExtension},
};
use proton_mail_common::models::{Conversation, Label, Message};
use proton_mail_test_utils::common::TestContext;

#[tokio::test]
async fn unsynced_conversations() {
    let ctx = TestContext::new().await;
    let user_context = ctx.user_context().await;
    let stash = user_context.user_stash();
    let api = user_context.session().api();

    ctx.mock_get_labels_by_ids(ctx.get_test_labels()).await;
    ctx.mock_get_conversations(ctx.get_test_convers(), 1).await;

    let options = GetConversationsOptions::default();
    Conversation::search(options, api, stash)
        .await
        .expect("Error searching for conversations");

    // Now all of the labels should exist!
    Label::find_by_id(RemoteId::from("Label1"), stash)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_id(RemoteId::from("Label2"), stash)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_id(RemoteId::from("Label3"), stash)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn unsynced_messages() {
    let ctx = TestContext::new().await;
    let user_context = ctx.user_context().await;
    let stash = user_context.user_stash();
    let api = user_context.session().api();

    ctx.mock_get_labels_by_ids(ctx.get_test_labels()).await;
    let addrs = ctx.get_test_addrs();
    ctx.mock_get_address(addrs[0].clone()).await;
    ctx.mock_get_address(addrs[1].clone()).await;
    ctx.mock_get_messages(ctx.get_test_msgs()).await;

    let options = GetMessagesOptions::default();
    Message::search(options, api, stash)
        .await
        .expect("Error searching for messages");

    // Now all of the labels should exist!
    Label::find_by_id(RemoteId::from("Label1"), stash)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_id(RemoteId::from("Label2"), stash)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_id(RemoteId::from("Label3"), stash)
        .await
        .unwrap()
        .unwrap();
    Address::find_by_id(RemoteId::from("Addr1"), stash)
        .await
        .unwrap()
        .unwrap();
    Address::find_by_id(RemoteId::from("Addr2"), stash)
        .await
        .unwrap()
        .unwrap();
}
