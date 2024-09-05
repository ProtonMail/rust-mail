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
use std::fs;

#[tokio::test]
async fn get_sender_image() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new().await;
    let mut params = TestParams::default_basic();
    let user_context = ctx.user_context().await;
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
    ctx.mock_get_conversations(conversations, 1_u64).await;
    ctx.mock_get_image_for_conversation(b"abcdef".to_vec())
        .await;
    ctx.catch_all().await;
    user_context
        .initialize_async(&NullCallback {})
        .await
        .expect("failed to initialize");

    // Create a mailbox
    let mailbox = Mailbox::with_remote_id(user_context.clone(), LabelId::inbox())
        .await
        .unwrap();

    mailbox.sync(1).await.expect("mailbox sync failed");
    let local_conversation = Conversation::find_first("", vec![], user_context.user_stash())
        .await
        .unwrap()
        .unwrap();
    let sender = &local_conversation.senders.value.first().unwrap();
    let mail_settings = MailSettings::load(MAIL_SETTINGS_ID.into(), user_context.user_stash())
        .await
        .expect("failed to load mail settings")
        .unwrap();

    let image_path = user_context
        .image_for_sender(
            &mail_settings,
            sender.address.clone(),
            sender.bimi_selector.as_deref(),
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
