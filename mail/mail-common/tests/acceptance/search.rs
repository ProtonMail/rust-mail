use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_common::models::{Address, Label, ModelIdExtension};
use proton_mail_api::services::proton::requests::{GetConversationsOptions, GetMessagesOptions};
use proton_mail_common::models::{Conversation, Message};
use proton_mail_common::test_utils::test_context::MailTestContext;

#[tokio::test]
async fn unsynced_conversations() {
    let ctx = MailTestContext::new().await;
    let user_context = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_context.user_stash().connection().await.unwrap();
    let api = user_context.session();

    ctx.mock_get_labels_by_ids(ctx.get_test_labels()).await;
    ctx.mock_get_conversations(ctx.get_test_convers(), 1).await;

    let options = GetConversationsOptions::default();
    Conversation::search(options, api, &mut tether)
        .await
        .expect("Error searching for conversations");

    // Now all of the labels should exist!
    Label::find_by_remote_id(LabelId::from("Label1"), &tether)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_remote_id(LabelId::from("Label2"), &tether)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_remote_id(LabelId::from("Label3"), &tether)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn unsynced_messages() {
    let ctx = MailTestContext::new().await;
    let user_context = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_context.user_stash().connection().await.unwrap();
    let api = user_context.session();

    ctx.mock_get_labels_by_ids(ctx.get_test_labels()).await;

    let addrs = ctx.get_test_addrs();

    ctx.core_test_context
        .mock_get_address(addrs[0].clone())
        .await;

    ctx.core_test_context
        .mock_get_address(addrs[1].clone())
        .await;

    ctx.mock_get_messages()
        .respond_with(ctx.get_test_msgs())
        .await;

    let options = GetMessagesOptions::default();

    Message::search(options, api, &mut tether)
        .await
        .expect("Error searching for messages");

    // Now all of the labels should exist!
    Label::find_by_remote_id(LabelId::from("Label1"), &tether)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_remote_id(LabelId::from("Label2"), &tether)
        .await
        .unwrap()
        .unwrap();
    Label::find_by_remote_id(LabelId::from("Label3"), &tether)
        .await
        .unwrap()
        .unwrap();
    Address::find_by_remote_id(AddressId::from("Addr1"), &tether)
        .await
        .unwrap()
        .unwrap();
    Address::find_by_remote_id(AddressId::from("Addr2"), &tether)
        .await
        .unwrap()
        .unwrap();
}
