mod common;

use crate::common::attachment::{testdata_attachment_data, testdata_expected_attachment_decrypted};
use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_core_common::datatypes::LabelId;
use proton_crypto_inbox::proton_crypto::crypto::VerificationError;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Attachment, Conversation};
use proton_mail_common::Mailbox;
use stash::orm::Model;
use stash::params;

#[tokio::test]
#[ignore]
async fn test_load_attachment_buffer() {
    let ctx = TestContext::new().await;
    let params = TestParams::default_basic();

    // Api mock.
    let conversations = params.conversations.clone();
    let test_attachment = params.attachments.first().unwrap();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 1).await;
    ctx.mock_get_attachment_metadata(test_attachment.clone())
        .await;
    ctx.mock_get_attachment_data(test_attachment.id.clone(), testdata_attachment_data())
        .await;
    ctx.catch_all().await;
    ctx.user_context()
        .await
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");
    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(ctx.user_context().await, LabelId::inbox())
        .await
        .unwrap();

    // Sync mails.
    mailbox.sync(1).await.expect("mailbox sync failed");

    // Get default conversation with the default attachment.
    let local_conversation = Conversation::find_first("", vec![], ctx.user_context().await.stash())
        .await
        .expect("failed to load conversation")
        .unwrap();
    let attachment_remote_id = local_conversation
        .attachments_metadata
        .value
        .first()
        .unwrap()
        .remote_id
        .clone();
    let attachment_local_id = Attachment::find_first(
        "WHERE remote_id = ?",
        params![attachment_remote_id],
        ctx.user_context().await.stash(),
    )
    .await
    .expect("failed to load attachment")
    .unwrap()
    .local_id
    .unwrap();
    // Load and decrypt attachment.
    let decryption_result = mailbox
        .load_attachment_to_buffer(attachment_local_id)
        .await
        .expect("decryption should not fail");
    assert_eq!(
        decryption_result.content,
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
    assert!(
        matches!(
            decryption_result.verification_result,
            Err(VerificationError::NotSigned(_))
        ),
        "There should be no signatures to verify"
    );
}
