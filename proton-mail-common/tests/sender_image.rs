mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{Label, LabelId, LabelType};
use proton_mail_common::Mailbox;

#[test]
fn test_get_sender_image() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new();
    let mut params = TestParams::default_basic();
    params
        .labels
        .get_mut(&LabelType::Label)
        .unwrap()
        .push(Label {
            id: LabelId::from("testlabel"),
            parent_id: None,
            name: "testlabel".to_string(),
            path: None,
            color: "".to_string(),
            label_type: LabelType::Label,
            notify: false,
            display: false,
            sticky: false,
            expanded: false,
            order: 0,
        });
    ctx.async_runtime().block_on(async {
        let conversations = params.conversations.clone();
        ctx.setup_user(params.clone()).await;
        ctx.mock_get_conversations(conversations, 1).await;
        ctx.mock_get_image_for_conversation(vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07])
            .await;
        ctx.catch_all().await;
        ctx.user_context()
            .initialize_async(LabelId::inbox().clone(), &NullCallback {})
            .await
            .expect("failed to initialize");
    });

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(ctx.user_context(), LabelId::inbox()).unwrap();

    ctx.async_runtime().block_on(async {
        mailbox.sync(1).await.expect("mailbox sync failed");
    });
    let local_conversation = mailbox.conversations(2).unwrap();
    let senders = &local_conversation.first().unwrap().senders;

    ctx.async_runtime().block_on(async {
        let image = ctx
            .user_context()
            .image_for_senders(senders, None, None, None)
            .await
            .expect("failed to get image")
            .expect("should have value");
        assert_eq!(
            image.to_vec(),
            vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]
        )
    });
}
