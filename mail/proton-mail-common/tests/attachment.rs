mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::LabelId;
use proton_mail_common::Mailbox;

use crate::common::attachment::test_expected_attachment_decrypted;

#[ignore]
#[test]
fn test_load_attachment_buffer() {
    let ctx = TestContext::new();
    let params = TestParams::default_basic();

    ctx.async_runtime().block_on(async {
        let conversations = params.conversations.clone();
        ctx.setup_user(params.clone()).await;
        ctx.mock_get_conversations(conversations, 1).await;
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
    let local_conversation = mailbox.conversations(1).unwrap();
    let (attachment, _verification_result) = mailbox
        .load_attachment_to_buffer(
            local_conversation
                .first()
                .unwrap()
                .attachments
                .as_ref()
                .unwrap()
                .first()
                .unwrap()
                .id,
        )
        .unwrap();
    assert_eq!(
        attachment,
        test_expected_attachment_decrypted(),
        "attachments should be equal"
    )
}
