use proton_core_api::services::proton::Label as ApiLabel;
use proton_core_api::services::proton::LabelId;
use proton_core_api::services::proton::LabelType as ApiLabelType;
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Conversation;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use stash::orm::Model;
use std::fs;

#[tokio::test]
async fn get_sender_image() {
    // Set up a user and initialise the inbox
    let ctx = MailTestContext::new().await;
    let mut params = TestParams::default_basic();
    params
        .labels
        .get_mut(&ApiLabelType::Label)
        .unwrap()
        .push(ApiLabel {
            id: LabelId::from("testlabel"),
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
    let user_ctx = ctx.mail_user_context().await;

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(&user_ctx.user_stash().connection(), LabelId::inbox())
        .await
        .unwrap();

    mailbox
        .sync(
            &mut user_ctx.user_stash().connection(),
            user_ctx.session(),
            1,
        )
        .await
        .expect("mailbox sync failed");
    let tether = user_ctx.user_stash().connection();
    let local_conversation = Conversation::find_first("", vec![], &tether)
        .await
        .unwrap()
        .unwrap();
    let sender = &local_conversation.senders.value.first().unwrap();
    let image_path = user_ctx
        .image_for_sender(
            sender.address.clone(),
            sender.bimi_selector.clone(),
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
