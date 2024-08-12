mod common;

use crate::common::attachment::{testdata_attachment_data, testdata_expected_attachment_decrypted};
use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::services::proton::response_data::Attachment as ApiAttachment;
use proton_core_common::datatypes::{LabelId, LocalId};
use proton_mail_common::datatypes::{attachment, Disposition, SystemLabelId};
use proton_mail_common::models::{Attachment, Conversation};
use proton_mail_common::Mailbox;
use stash::orm::Model;
use std::fs;

#[tokio::test]
#[ignore]
async fn test_load_attachment_buffer() {
    let ctx = TestContext::new().await;
    let params = TestParams::default_basic();
    let user_context = ctx.user_context().await;

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
    user_context
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");
    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(user_context.clone(), LabelId::inbox())
        .await
        .unwrap();

    // Sync mails.
    mailbox.sync(1).await.expect("mailbox sync failed");

    // Get default conversation with the default attachment.
    let local_conversation = Conversation::find_first("", vec![], user_context.stash())
        .await
        .expect("failed to load conversation")
        .unwrap();

    let attachment_local_id = local_conversation.local_id.unwrap();

    // Cache is empty
    assert!(user_context.attachements_cache().is_empty());

    // Load and decrypt attachment.
    let decryption_result = mailbox
        .load_attachment_to_buffer(attachment_local_id)
        .await
        .expect("decryption should not fail");
    assert_eq!(
        fs::read(decryption_result.data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
    assert_eq!(user_context.attachements_cache().len(), 1);
    mailbox
        .load_attachment_to_buffer(attachment_local_id)
        .await
        .expect("decryption should not fail");
    assert_eq!(user_context.attachements_cache().len(), 1);
}

#[tokio::test]
#[ignore]
async fn load_attachment_from_cache() {
    let ctx = TestContext::new().await;
    let params = TestParams::default_basic();
    let user_context = ctx.user_context().await;

    // Api mock.
    let conversations = params.conversations.clone();
    let test_attachment = params.attachments.first().unwrap();
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 1).await;
    ctx.mock_get_attachment_metadata(test_attachment.clone())
        .await;
    ctx.catch_all().await;
    user_context
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");
    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(user_context.clone(), LabelId::inbox())
        .await
        .unwrap();

    // Sync mails.
    mailbox.sync(1).await.expect("mailbox sync failed");

    // Get default conversation with the default attachment.
    let local_conversation = Conversation::find_first("", vec![], user_context.stash())
        .await
        .expect("failed to load conversation")
        .unwrap();
    let attachment_local_id = local_conversation.local_id.unwrap();

    // Add another value into cache
    user_context
        .attachements_cache()
        .add_item(attachment_local_id.into(), &testdata_attachment_data())
        .unwrap();

    // Load and decrypt attachment.
    let decryption_result = mailbox
        .load_attachment_to_buffer(attachment_local_id)
        .await
        .expect("decryption should not fail");
    assert_eq!(
        fs::read(decryption_result.data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
}

#[tokio::test]
// TODO: work with https://gitlab.protontech.ch/rust/proton-api-core/-/merge_requests/92
async fn load_attachment_content_first_time() {
    // Setup
    //   * Create an attachment
    //   * Check cache is empty
    let ctx = TestContext::new().await;
    let params = TestParams::default_basic();
    let user_context = ctx.user_context().await;
    let test_attachment = params.attachments.first().unwrap();
    let mut attachment: Attachment = test_attachment.clone().try_into().unwrap();
    let attachment_local_id = 42;
    attachment.local_id = Some(attachment_local_id.into());

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_attachment_data(test_attachment.id.clone(), testdata_attachment_data())
        .await;
    ctx.catch_all().await;
    user_context
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    let mailbox = Mailbox::with_remote_id(user_context.clone(), LabelId::inbox())
        .await
        .unwrap();

    ctx.catch_all().await;

    assert!(user_context.attachements_cache().is_empty());

    // Action:
    //   * Get attachment
    let data_path = mailbox
        .get_attachment_content(attachment_local_id.into(), &attachment)
        .await
        .unwrap();

    // Validate:
    //   * attachment is the decrypted one
    //   * cache contain an item now
    assert_eq!(
        fs::read(data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
    assert_eq!(user_context.attachements_cache().len(), 1);
}

#[tokio::test]
// TODO: work with https://gitlab.protontech.ch/rust/proton-api-core/-/merge_requests/92
async fn load_attachment_content_from_cache() {
    // Setup
    //   * Create an attachment
    //   * Add attachment data into cache
    let ctx = TestContext::new().await;
    let params = TestParams::default_basic();
    let user_context = ctx.user_context().await;
    let test_attachment = params.attachments.first().unwrap();
    let attachment_local_id = 42;
    let attachment = get_attachment(attachment_local_id.into(), test_attachment);

    ctx.setup_user(params.clone()).await;
    ctx.catch_all().await;
    user_context
        .initialize_async(LabelId::inbox().clone(), &NullCallback {})
        .await
        .expect("failed to initialize");

    let mailbox = Mailbox::with_remote_id(user_context.clone(), LabelId::inbox())
        .await
        .unwrap();

    ctx.catch_all().await;

    user_context
        .attachements_cache()
        .add_item(attachment_local_id, b"abcdef")
        .unwrap();

    // Action:
    //   * Get attachment
    let data_path = mailbox
        .get_attachment_content(attachment_local_id.into(), &attachment)
        .await
        .unwrap();

    // Validate:
    //   * attachment is the same as the one in cache
    assert_eq!(
        fs::read(data_path).unwrap(),
        b"abcdef",
        "attachments should be equal"
    );
}

fn get_attachment(id: LocalId, attachment: &ApiAttachment) -> Attachment {
    Attachment {
        local_id: Some(id),
        remote_id: Some(attachment.id.clone().into()),
        remote_address_id: Some(attachment.address_id.clone().into()),
        local_conversation_id: None,
        remote_conversation_id: Some(attachment.conversation_id.clone().into()),
        local_message_id: None,
        remote_message_id: Some(attachment.message_id.clone().into()),
        disposition: Disposition::Attachment,
        enc_signature: None,
        is_auto_forwardee: false,
        key_packets: Some(attachment.key_packets.clone().into()),
        mime_type: attachment::MimeType::new(&attachment.mime_type).unwrap(),
        filename: attachment.name.clone(),
        sender: None,
        signature: None,
        size: attachment.size,
        row_id: None,
        stash: None,
    }
}
