mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{LabelId, LabelType};
use proton_mail_common::Mailbox;

#[test]
fn test_new_mailbox() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new();
    let params = TestParams::default_basic();
    ctx.async_runtime().block_on(async {
        let conversations = params.conversations.clone();
        ctx.setup_user(params.clone()).await;
        ctx.mock_get_conversations(conversations).await;
        ctx.user_context()
            .initialize_async(LabelId::inbox().clone(), &NullCallback {})
            .await
            .expect("failed to initialize");
    });
    ctx.catch_all();

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(
        ctx.user_context(),
        &params.labels.get(&LabelType::Label).unwrap()[0].id,
    )
    .unwrap();

    // Sync the mailbox
    mailbox.conversations(10).unwrap();
    ctx.async_runtime().block_on(async {
        mailbox.sync(10, None).unwrap();
    });
}
