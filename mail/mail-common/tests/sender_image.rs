use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_core_common::datatypes::LabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Conversation;
use proton_mail_common::Mailbox;
use proton_mail_test_utils::common::TestContext;
use proton_mail_test_utils::init::Params as TestParams;
use stash::orm::Model;
use std::fs;

#[tokio::test]
async fn get_sender_image() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new().await;
    let mut params = TestParams::default_basic();
    let user_ctx = ctx.user_context().await;
    params
        .labels
        .get_mut(&ApiLabelType::Label)
        .unwrap()
        .push(ApiLabel {
            id: ApiRemoteId::from("testlabel"),
            parent_id: None,
            name: "testlabel".to_owned(),
            path: None,
            color: String::new(),
            label_type: ApiLabelType::Label,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        });

    let conversations = params.conversations.clone();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_get_image_for_conversation(b"abcdef".to_vec())
        .await;
    ctx.catch_all().await;
    ctx.init_user(user_ctx.clone()).await;

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(user_ctx.clone(), LabelId::inbox())
        .await
        .unwrap();

    mailbox.sync(1).await.expect("mailbox sync failed");
    let local_conversation = Conversation::find_first("", vec![], user_ctx.user_stash())
        .await
        .unwrap()
        .unwrap();
    let sender = &local_conversation.senders.value.first().unwrap();
    let image_path = user_ctx
        .image_for_sender(
            sender.address.clone(),
            sender.bimi_selector.as_deref(),
            sender.display_sender_image,
            None,
            None,
            None,
        )
        .await
        .expect("failed to get image")
        .expect("should have value");
    assert_eq!(fs::read(image_path).unwrap(), b"abcdef");
}
