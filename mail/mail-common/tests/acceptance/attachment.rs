use futures::future::try_join_all;
use proton_core_api::services::proton::LabelId;
use proton_mail_common::datatypes::LocalAttachmentId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Attachment, Conversation};
use proton_mail_common::test_utils::attachment::{
    testdata_attachment_data, testdata_expected_attachment_decrypted,
};
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::MailTestContext;
use proton_mail_common::{DecryptedAttachment, Mailbox};
use stash::orm::Model;
use std::fs;
use std::path::PathBuf;
use wiremock::Times;

async fn setup_common(
    ctx: &MailTestContext,
    params: &TestParams,
    metadata_mock_count: impl Into<Times>,
    data_mock_count: impl Into<Times>,
) -> LocalAttachmentId {
    let conversations = params.conversations.clone();
    let test_attachment = params.attachments.first().unwrap();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_conversations(conversations, 1).await;
    ctx.mock_get_attachment_metadata(test_attachment.clone(), metadata_mock_count)
        .await;
    ctx.mock_get_attachment_data(
        test_attachment.id.clone(),
        testdata_attachment_data(),
        data_mock_count,
    )
    .await;

    let user_ctx = ctx.mail_user_context().await;

    let mailbox = Mailbox::with_remote_id(
        &user_ctx.user_stash().connection().await.unwrap(),
        LabelId::inbox(),
    )
    .await
    .unwrap();

    mailbox
        .sync(
            &mut user_ctx.user_stash().connection().await.unwrap(),
            user_ctx.session(),
            1,
        )
        .await
        .expect("mailbox sync failed");

    let tether = user_ctx.user_stash().connection().await.unwrap();
    let local_conversation = Conversation::find_first("", vec![], &tether)
        .await
        .expect("failed to load conversation")
        .unwrap();

    local_conversation.attachments_metadata[0].local_id.unwrap()
}

#[tokio::test]
async fn test_load_attachment_buffer() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    let attachment_local_id = setup_common(&ctx, &params, 1, 1).await;
    let user_ctx = ctx.mail_user_context().await;

    // Load and decrypt attachment.
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let att = Attachment::get_attachment(&user_ctx, attachment_local_id, &mut tether)
        .await
        .expect("decryption should not fail");
    assert_eq!(
        fs::read(&att.data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
    filename_is_correct(&att);
    let att_again = Attachment::get_attachment(&user_ctx, attachment_local_id, &mut tether)
        .await
        .expect("decryption should not fail");
    assert_eq!(att, att_again);
}

#[tokio::test]
async fn concurrency() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    let attachment_local_id = setup_common(&ctx, &params, 1.., 1..).await;
    let user_ctx = ctx.mail_user_context().await;

    let requests = (0..30).map(|_| {
        let ctx_clone = user_ctx.clone();
        async move {
            let mut tether = ctx_clone.user_stash().connection().await.unwrap();
            Attachment::get_attachment(&ctx_clone, attachment_local_id, &mut tether).await
        }
    });

    let mut result = try_join_all(requests).await.unwrap();
    let mut last = result.pop().unwrap();
    while let Some(next) = result.pop() {
        assert_eq!(next, last);
        last = next;
    }
    filename_is_correct(&last);
    // Load and decrypt attachment.
    assert_eq!(
        fs::read(&last.data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
}

#[tokio::test]
async fn load_attachment_from_cache() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    let attachment_local_id = setup_common(&ctx, &params, 1, 1).await;
    let user_ctx = ctx.mail_user_context().await;

    // Load and decrypt attachment.
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let decryption_result = Attachment::get_attachment(&user_ctx, attachment_local_id, &mut tether)
        .await
        .expect("decryption should not fail");

    filename_is_correct(&decryption_result);
    assert_eq!(
        fs::read(&decryption_result.data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
}

#[tokio::test]
async fn external_attachment_file_removal_from_cache_triggers_download() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();

    let attachment_local_id = setup_common(&ctx, &params, 1, 2).await;
    let user_ctx = ctx.mail_user_context().await;

    // Load and decrypt attachment.
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let decryption_result = Attachment::get_attachment(&user_ctx, attachment_local_id, &mut tether)
        .await
        .expect("decryption should not fail");

    filename_is_correct(&decryption_result);
    assert_eq!(
        fs::read(&decryption_result.data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );

    // if the attachment is no longer in disk we re-download it
    fs::remove_file(&decryption_result.data_path).unwrap();

    let decryption_result = Attachment::get_attachment(&user_ctx, attachment_local_id, &mut tether)
        .await
        .expect("decryption should not fail");

    filename_is_correct(&decryption_result);
    assert_eq!(
        fs::read(decryption_result.data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
}

#[tokio::test]
async fn load_attachment_content_first_time() {
    // Setup
    //   * Create an attachment
    //   * Check cache is empty
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    let test_attachment = params.attachments.first().unwrap();
    let mut attachment: Attachment = test_attachment.clone().into();
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    tether
        .tx(async |tx| attachment.save(tx).await)
        .await
        .unwrap();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_attachment_data(test_attachment.id.clone(), testdata_attachment_data(), 1)
        .await;
    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Action:
    //   * Get attachment
    let data_path = attachment
        .content_path(&user_ctx, &mut tether)
        .await
        .unwrap();

    // Validate:
    //   * attachment is the decrypted one
    assert_eq!(
        fs::read(data_path).unwrap(),
        testdata_expected_attachment_decrypted(),
        "attachments should be equal"
    );
}

fn filename_is_correct(at: &DecryptedAttachment) {
    let path = PathBuf::from(&at.data_path);
    let path_name = path.file_name().unwrap().to_string_lossy();
    assert_eq!(path_name, at.attachment_metadata.filename);
}
