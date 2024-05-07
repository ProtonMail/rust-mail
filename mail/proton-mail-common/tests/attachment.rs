mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::LabelId;
use proton_crypto_inbox::proton_crypto::crypto::VerificationError;
use proton_mail_common::Mailbox;

use crate::common::attachment::{test_attachment_data, test_expected_attachment_decrypted};

#[test]
fn test_load_attachment_buffer() {
    let ctx = TestContext::new();
    let params = TestParams::default_basic();

    // Api mock.
    ctx.async_runtime().block_on(async {
        let conversations = params.conversations.clone();
        let test_attachment = params.attachments.first().unwrap();
        ctx.setup_user(params.clone()).await;
        ctx.mock_get_conversations(conversations, 1).await;
        ctx.mock_get_attachment_metadata(test_attachment.clone())
            .await;
        ctx.mock_get_attachment_data(test_attachment.id.clone(), test_attachment_data())
            .await;
        ctx.catch_all().await;
        ctx.user_context()
            .initialize_async(LabelId::inbox().clone(), &NullCallback {})
            .await
            .expect("failed to initialize");
    });
    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(ctx.user_context(), LabelId::inbox()).unwrap();

    // Sync mails.
    ctx.async_runtime().block_on(async {
        mailbox.sync(1).await.expect("mailbox sync failed");
    });

    // Get default conversation with the default attachment.
    let local_conversation = mailbox.conversations(1).unwrap();
    let attachment_id = local_conversation
        .first()
        .unwrap()
        .attachments
        .as_ref()
        .unwrap()
        .first()
        .unwrap()
        .id;
    // Load and decrypt attachment.
    let (attachment, verification_result) =
        mailbox.load_attachment_to_buffer(attachment_id).unwrap();
    assert_eq!(
        attachment,
        test_expected_attachment_decrypted(),
        "attachments should be equal"
    );
    assert!(
        matches!(verification_result, Err(VerificationError::NotSigned(_))),
        "There should be no signatures to verify"
    );
}
