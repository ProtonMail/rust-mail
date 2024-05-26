mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{Label, LabelId, LabelType};
use proton_mail_common::Mailbox;

#[tokio::test]
async fn test_new_mailbox() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new().await;
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
    let conversations = params.conversations.clone();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 2).await;
    ctx.catch_all().await;
    ctx.user_context()
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    // Create a mailbox
    let mailbox1 = Mailbox::with_remote_id(
        ctx.user_context(),
        &params.labels.get(&LabelType::Label).unwrap()[0].id,
    )
    .unwrap();

    // Create another mailbox
    let mailbox2 = Mailbox::with_remote_id(
        ctx.user_context(),
        &params.labels.get(&LabelType::Label).unwrap()[1].id,
    )
    .unwrap();

    // Sync mailbox 1 - this should fire a network request
    mailbox1.sync(10).await.unwrap();

    // Sync mailbox 2 - this should also fire a network request
    mailbox2.sync(10).await.unwrap();

    // Try syncing mailbox1 again - this should not fire any network requests
    mailbox1.sync(10).await.unwrap();

    // Try syncing mailbox2 again - this should not fire any network requests
    mailbox2.sync(10).await.unwrap();
}
