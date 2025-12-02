use proton_core_api::services::proton::LabelId;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::response_data::{
    MailSettings as ApiMailSettings, MessageMetadata as ApiMessageMetadata, ViewMode as ApiViewMode,
};
use proton_mail_common::Mailbox;
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::models::Message;
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::test_context::MailTestContext;
use stash::orm::Model;
use test_case::test_case;

#[tokio::test]
async fn bulk_unread_status_empty_list() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let result = Message::bulk_unread_status_by_remote_ids(vec![], &tether)
        .await
        .unwrap();

    assert!(result.is_empty());
}

#[tokio::test]
async fn bulk_unread_status_nonexistent_messages() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let remote_ids = vec![
        MessageId::from("nonexistent1"),
        MessageId::from("nonexistent2"),
        MessageId::from("nonexistent3"),
    ];

    let result = Message::bulk_unread_status_by_remote_ids(remote_ids, &tether)
        .await
        .unwrap();

    assert_eq!(result, vec![true, true, true]);
}

#[test_case(vec![true, true, true], vec![true, true, true]; "all unread")]
#[test_case(vec![false, false, false], vec![false, false, false]; "all read")]
#[test_case(vec![true, false, true], vec![true, false, true]; "mixed")]
#[tokio::test]
async fn bulk_unread_status_existing_messages(unread_states: Vec<bool>, expected: Vec<bool>) {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut params = Params::default_basic();
    params.mail_settings = Some(ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    });
    params.message_count[0].total = unread_states.len() as u64;

    let messages: Vec<_> = unread_states
        .iter()
        .enumerate()
        .map(|(i, &unread)| {
            let message_id = format!("msg{}", i + 1);
            ApiMessageMetadata {
                id: MessageId::from(message_id),
                conversation_id: params.conversations[0].id.clone(),
                address_id: params.addresses[0].id.clone(),
                unread,
                ..ApiMessageMetadata::test_default()
            }
        })
        .collect();

    let remote_ids: Vec<MessageId> = messages.iter().map(|m| m.id.clone()).collect();

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages().respond_with(messages).await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let result = Message::bulk_unread_status_by_remote_ids(remote_ids, &tether)
        .await
        .unwrap();

    assert_eq!(result, expected);
}

#[tokio::test]
async fn bulk_unread_status_mixed_existing_and_nonexistent() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut params = Params::default_basic();
    params.mail_settings = Some(ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    });
    params.message_count[0].total = 2;

    let existing_messages = vec![
        ApiMessageMetadata {
            id: MessageId::from("existing1"),
            conversation_id: params.conversations[0].id.clone(),
            address_id: params.addresses[0].id.clone(),
            unread: true,
            ..ApiMessageMetadata::test_default()
        },
        ApiMessageMetadata {
            id: MessageId::from("existing2"),
            conversation_id: params.conversations[0].id.clone(),
            address_id: params.addresses[0].id.clone(),
            unread: false,
            ..ApiMessageMetadata::test_default()
        },
    ];

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages()
        .respond_with(existing_messages)
        .await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let remote_ids = vec![
        MessageId::from("existing1"),
        MessageId::from("nonexistent1"),
        MessageId::from("existing2"),
        MessageId::from("nonexistent2"),
    ];

    let result = Message::bulk_unread_status_by_remote_ids(remote_ids, &tether)
        .await
        .unwrap();

    assert_eq!(result, vec![true, true, false, true]);
}

#[tokio::test]
async fn bulk_unread_status_preserves_order() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut params = Params::default_basic();
    params.mail_settings = Some(ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    });
    params.message_count[0].total = 3;

    let messages = vec![
        ApiMessageMetadata {
            id: MessageId::from("msg1"),
            conversation_id: params.conversations[0].id.clone(),
            address_id: params.addresses[0].id.clone(),
            unread: true,
            ..ApiMessageMetadata::test_default()
        },
        ApiMessageMetadata {
            id: MessageId::from("msg2"),
            conversation_id: params.conversations[0].id.clone(),
            address_id: params.addresses[0].id.clone(),
            unread: false,
            ..ApiMessageMetadata::test_default()
        },
        ApiMessageMetadata {
            id: MessageId::from("msg3"),
            conversation_id: params.conversations[0].id.clone(),
            address_id: params.addresses[0].id.clone(),
            unread: true,
            ..ApiMessageMetadata::test_default()
        },
    ];

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages().respond_with(messages).await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let remote_ids_forward = vec![
        MessageId::from("msg1"),
        MessageId::from("msg2"),
        MessageId::from("msg3"),
    ];
    let remote_ids_reverse = vec![
        MessageId::from("msg3"),
        MessageId::from("msg2"),
        MessageId::from("msg1"),
    ];

    let result_forward = Message::bulk_unread_status_by_remote_ids(remote_ids_forward, &tether)
        .await
        .unwrap();
    let result_reverse = Message::bulk_unread_status_by_remote_ids(remote_ids_reverse, &tether)
        .await
        .unwrap();

    assert_eq!(result_forward, vec![true, false, true]);
    assert_eq!(result_reverse, vec![true, false, true]);
}

#[tokio::test]
async fn bulk_unread_status_with_duplicates() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut params = Params::default_basic();
    params.mail_settings = Some(ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    });
    params.message_count[0].total = 2;

    let messages = vec![
        ApiMessageMetadata {
            id: MessageId::from("msg1"),
            conversation_id: params.conversations[0].id.clone(),
            address_id: params.addresses[0].id.clone(),
            unread: true,
            ..ApiMessageMetadata::test_default()
        },
        ApiMessageMetadata {
            id: MessageId::from("msg2"),
            conversation_id: params.conversations[0].id.clone(),
            address_id: params.addresses[0].id.clone(),
            unread: false,
            ..ApiMessageMetadata::test_default()
        },
    ];

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages().respond_with(messages).await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let remote_ids_with_duplicates = vec![
        MessageId::from("msg1"),
        MessageId::from("msg2"),
        MessageId::from("msg1"),
        MessageId::from("msg2"),
    ];

    let result = Message::bulk_unread_status_by_remote_ids(remote_ids_with_duplicates, &tether)
        .await
        .unwrap();

    assert_eq!(result, vec![true, false, true, false]);
}

#[tokio::test]
async fn bulk_unread_status_ignores_deleted_messages() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    let mut params = Params::default_basic();
    params.mail_settings = Some(ApiMailSettings {
        view_mode: ApiViewMode::Messages,
        ..Default::default()
    });
    params.message_count[0].total = 1;

    let message = ApiMessageMetadata {
        id: MessageId::from("msg1"),
        conversation_id: params.conversations[0].id.clone(),
        address_id: params.addresses[0].id.clone(),
        unread: true,
        ..ApiMessageMetadata::test_default()
    };

    ctx.setup_user(params.clone()).await;
    ctx.mock_get_messages().respond_with(vec![message]).await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let mailbox = Mailbox::with_remote_id(&tether, LabelId::inbox())
        .await
        .unwrap();
    mailbox
        .sync(&mut tether, user_ctx.session(), 10)
        .await
        .unwrap();

    let mut msg = Message::find_by_remote_id(MessageId::from("msg1"), &tether)
        .await
        .unwrap()
        .unwrap();
    msg.deleted = true;
    tether.tx(async |bond| msg.save(bond).await).await.unwrap();

    let remote_ids = vec![MessageId::from("msg1")];
    let result = Message::bulk_unread_status_by_remote_ids(remote_ids, &tether)
        .await
        .unwrap();

    assert_eq!(result, vec![true]);
}
