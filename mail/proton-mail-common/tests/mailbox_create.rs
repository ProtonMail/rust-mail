mod common;

use common::init::{NullCallback, Params as TestParams};
use common::TestContext;
use proton_api_mail::domain::{
    Label, LabelId, LabelType, MailSettings, MailSettingsViewMode, MessageFlags, MessageId,
    MessageMetadata,
};
use proton_mail_common::Mailbox;

#[tokio::test]
async fn test_new_mailbox_sync_conversations() {
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
    ctx.async_runtime().block_on(async {
        mailbox1.sync(10).await.unwrap();
    });

    // Sync mailbox 2 - this should also fire a network request
    ctx.async_runtime().block_on(async {
        mailbox2.sync(10).await.unwrap();
    });

    // Try syncing mailbox1 again - this should not fire any network requests
    ctx.async_runtime().block_on(async {
        mailbox1.sync(10).await.unwrap();
    });

    // Try syncing mailbox2 again - this should not fire any network requests
    ctx.async_runtime().block_on(async {
        mailbox2.sync(10).await.unwrap();
    });
}
#[test]
fn test_new_mailbox_sync_messages() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new();
    let mut params = TestParams::default_basic();
    let mut mail_settings = MailSettings::default();
    mail_settings.view_mode = MailSettingsViewMode::Messages;
    params.mail_settings = Some(mail_settings);

    let messages = vec![MessageMetadata {
        id: MessageId::from("MyMessageId"),
        conversation_id: params.conversations[0].id.clone(),
        order: 0,
        address_id: params.addresses[0].id.clone(),
        label_ids: vec![LabelId::inbox().clone()],
        external_id: None,
        subject: "foo".to_owned(),
        sender: Default::default(),
        to_list: vec![],
        cc_list: vec![],
        bcc_list: vec![],
        reply_tos: vec![],
        flags: MessageFlags::empty(),
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
    ctx.async_runtime().block_on(async {
        ctx.setup_user(params.clone()).await;
        ctx.mock_get_message_metadata(messages, 2).await;
        ctx.catch_all().await;
        ctx.user_context()
            .initialize_async(LabelId::inbox().clone(), &NullCallback {})
            .await
            .expect("failed to initialize");
    });

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

#[test]
fn test_new_mailbox_always_sync_messages_for_drafts_and_sent() {
    // Set up a user and initialise the inbox
    let ctx = TestContext::new();
    let mut params = TestParams::default_basic();
    // For view mode to conversations.
    let mut mail_settings = MailSettings::default();
    mail_settings.view_mode = MailSettingsViewMode::Conversations;
    params.mail_settings = Some(mail_settings);

    let messages = vec![MessageMetadata {
        id: MessageId::from("MyMessageId"),
        conversation_id: params.conversations[0].id.clone(),
        order: 0,
        address_id: params.addresses[0].id.clone(),
        label_ids: vec![LabelId::drafts().clone(), LabelId::sent().clone()],
        external_id: None,
        subject: "foo".to_owned(),
        sender: Default::default(),
        to_list: vec![],
        cc_list: vec![],
        bcc_list: vec![],
        reply_tos: vec![],
        flags: MessageFlags::empty(),
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
    ctx.async_runtime().block_on(async {
        ctx.setup_user(params.clone()).await;
        ctx.mock_get_message_metadata(messages, 2).await;
        ctx.catch_all().await;
        ctx.user_context()
            .initialize_async(LabelId::inbox().clone(), &NullCallback {})
            .await
            .expect("failed to initialize");
    });

    // Create a drafts mailbox
    let mailbox_drafts = Mailbox::with_remote_id(ctx.user_context(), LabelId::drafts()).unwrap();

    // Create sent mailbox
    let mailbox_sent = Mailbox::with_remote_id(ctx.user_context(), LabelId::sent()).unwrap();

    // Check that mailboxes always sync messages.
    ctx.async_runtime().block_on(async {
        mailbox_drafts.sync(10).await.unwrap();
    });

    ctx.async_runtime().block_on(async {
        mailbox_sent.sync(10).await.unwrap();
    });
}
