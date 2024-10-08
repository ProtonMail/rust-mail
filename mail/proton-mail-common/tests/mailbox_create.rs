mod common;

use common::init::Params as TestParams;
use common::TestContext;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::common::LabelType as ApiLabelType;
use proton_api_mail::services::proton::response_data::{
    Label as ApiLabel, MailSettings as ApiMailSettings, MessageFlags as ApiMessageFlags,
    MessageMetadata as ApiMessageMetadata, ViewMode as ApiViewMode,
};
use proton_core_common::datatypes::LabelId;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::Mailbox;

#[tokio::test]
#[ignore]
async fn test_new_mailbox_sync_conversations() {
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
    ctx.mock_get_conversations(conversations, 2_u64).await;
    ctx.catch_all().await;
    ctx.init_user(ctx.user_context().await).await;

    // Create a mailbox
    let mailbox1 = Mailbox::with_remote_id(
        ctx.user_context().await,
        params.labels.get(&ApiLabelType::Label).unwrap()[0]
            .id
            .clone()
            .into(),
    )
    .await
    .unwrap();

    // Create another mailbox
    let mailbox2 = Mailbox::with_remote_id(
        ctx.user_context().await,
        params.labels.get(&ApiLabelType::Label).unwrap()[1]
            .id
            .clone()
            .into(),
    )
    .await
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
#[tokio::test]
#[ignore]
async fn test_new_mailbox_sync_messages() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new().await;
    let mut params = TestParams::default_basic();
    let mut mail_settings = ApiMailSettings::default();
    mail_settings.view_mode = ApiViewMode::Messages;
    params.mail_settings = Some(mail_settings);

    let messages = vec![ApiMessageMetadata {
        id: ApiRemoteId::from("MyRemoteId"),
        conversation_id: params.conversations[0].id.clone(),
        order: 0,
        address_id: params.addresses[0].id.clone(),
        label_ids: vec![LabelId::inbox().into()],
        external_id: None,
        subject: "foo".to_owned(),
        sender: Default::default(),
        to_list: vec![],
        cc_list: vec![],
        bcc_list: vec![],
        reply_tos: vec![],
        flags: ApiMessageFlags::empty(),
        time: 0,
        size: 0,
        unread: false,
        is_replied: false,
        is_replied_all: false,
        is_forwarded: false,
        expiration_time: 0,
        snooze_time: 0,
        num_attachments: 0,
        attachments_metadata: vec![],
    }];

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
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message_metadata(messages, 2_u64).await;
    ctx.catch_all().await;
    ctx.init_user(ctx.user_context().await).await;

    // Create a mailbox
    let mailbox1 = Mailbox::with_remote_id(
        ctx.user_context().await,
        params.labels.get(&ApiLabelType::Label).unwrap()[0]
            .id
            .clone()
            .into(),
    )
    .await
    .unwrap();

    // Create another mailbox
    let mailbox2 = Mailbox::with_remote_id(
        ctx.user_context().await,
        params.labels.get(&ApiLabelType::Label).unwrap()[1]
            .id
            .clone()
            .into(),
    )
    .await
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

#[tokio::test]
#[ignore]
async fn test_new_mailbox_always_sync_messages_for_drafts_and_sent() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new().await;
    let mut params = TestParams::default_basic();
    // For view mode to conversations.
    let mut mail_settings = ApiMailSettings::default();
    mail_settings.view_mode = ApiViewMode::Conversations;
    params.mail_settings = Some(mail_settings);

    let messages = vec![ApiMessageMetadata {
        id: ApiRemoteId::from("MyRemoteId"),
        conversation_id: params.conversations[0].id.clone(),
        order: 0,
        address_id: params.addresses[0].id.clone(),
        label_ids: vec![LabelId::drafts().into(), LabelId::sent().into()],
        external_id: None,
        subject: "foo".to_owned(),
        sender: Default::default(),
        to_list: vec![],
        cc_list: vec![],
        bcc_list: vec![],
        reply_tos: vec![],
        flags: ApiMessageFlags::empty(),
        time: 0,
        size: 0,
        unread: false,
        is_replied: false,
        is_replied_all: false,
        is_forwarded: false,
        expiration_time: 0,
        snooze_time: 0,
        num_attachments: 0,
        attachments_metadata: vec![],
    }];

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
    ctx.setup_user(params.clone()).await;
    ctx.mock_get_message_metadata(messages, 2_u64).await;
    ctx.catch_all().await;
    ctx.init_user(ctx.user_context().await).await;

    // Create a drafts mailbox
    let mailbox_drafts = Mailbox::with_remote_id(ctx.user_context().await, LabelId::drafts())
        .await
        .unwrap();

    // Create sent mailbox
    let mailbox_sent = Mailbox::with_remote_id(ctx.user_context().await, LabelId::sent())
        .await
        .unwrap();

    // Check that mailboxes always sync messages.
    mailbox_drafts.sync(10).await.unwrap();

    mailbox_sent.sync(10).await.unwrap();
}
