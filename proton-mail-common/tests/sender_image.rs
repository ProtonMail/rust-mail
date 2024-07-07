mod common;

use crate::common::init::NullCallback;
use common::init::Params as TestParams;
use common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::Label as ApiLabel;
use proton_core_common::datatypes::LabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::{Conversation, MailSettings, MAIL_SETTINGS_ID};
use proton_mail_common::Mailbox;
use stash::orm::Model;

#[tokio::test]
async fn test_get_sender_image() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new().await;
    let mut params = TestParams::default_basic();
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
    ctx.mock_get_conversations(conversations, 1).await;
    ctx.mock_get_image_for_conversation(vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07])
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

    mailbox.sync(1).await.expect("mailbox sync failed");
    let local_conversation = Conversation::find_first("", vec![], ctx.user_context().await.stash())
        .await
        .unwrap()
        .unwrap();
    let sender = &local_conversation.senders.value.first().unwrap();
    let mail_settings = MailSettings::load(MAIL_SETTINGS_ID, ctx.user_context().await.stash())
        .await
        .expect("failed to load mail settings")
        .unwrap();

    let image = ctx
        .user_context()
        .await
        .image_for_sender(
            &mail_settings,
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
    assert_eq!(
        image.to_vec(),
        vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]
    )
}
