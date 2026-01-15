use std::sync::Arc;

use proton_action_queue::queue::{ActionError, AsActionError, QueuedError};
use proton_core_api::services::proton::common::ApiErrorInfo;
use proton_core_common::actions::event_poll::ActionEventLoopError;
use proton_core_common::datatypes::{Refresh, SystemLabel};
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_core_common::test_utils::account::test_api_address;
use proton_core_common::test_utils::addresses::MY_ADDRESS_ID;
use proton_event_loop::EventLoopError;
use proton_mail_api::services::proton::prelude::ViewMode;
use proton_mail_common::actions::refresh::ActionRefresh;
use proton_mail_common::models::{Conversation, DraftMetadata, Message};
use proton_mail_common::test_utils::init::{DEFAULT_MAIL_SETTINGS, Params as TestParams};
use proton_mail_common::test_utils::scroller::{
    StoreLabeledModelMap, UNIQUE_CONV_ID, create_single_message, test_conversations, test_messages,
};
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use proton_mail_common::{MailUserContext, api_conversation, api_message_meta};
use stash::orm::Model;
use velcro::hash_map;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate, Times};

async fn refresh(ctx: &MailUserContext, refresh: Refresh) -> Result<(), Arc<anyhow::Error>> {
    ctx.refresh_action(refresh).await.unwrap();
    let result = ctx.execute_all_actions().await;

    match result {
        Ok(_) => Ok(()),
        Err(QueuedError::Action(error, _id)) => Err(error),
        _ => panic!("Unexpected message: {result:?}"),
    }
}

fn create_error_response(code: u16, message: &str) -> ApiErrorInfo {
    ApiErrorInfo {
        code: code as u32,
        error: Some(message.to_string()),
        details: None,
    }
}

async fn setup_mail_refresh_mocks(ctx: &MailTestContext) {
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "1")), 1..)
        .await;
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "3")), 1..)
        .await;
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "4")), 1..)
        .await;
    ctx.mock_get_conversations_count(None, 1..).await;
    ctx.mock_get_messages_count(None, 1..).await;
    ctx.mock_get_mail_settings(None, 1..).await;
    ctx.mock_get_incoming_defaults(None, 1..).await;
}

async fn setup_contacts_refresh_mocks(ctx: &MailTestContext, expect: impl Into<Times> + Clone) {
    ctx.mock_get_contacts(None, expect.clone().into()).await;
    ctx.mock_get_contacts_emails(None, expect.into()).await;
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "2")), 1..)
        .await;
}

async fn setup_core_refresh_mocks(ctx: &MailTestContext) {
    setup_contacts_refresh_mocks(ctx, 1..).await;
    ctx.mock_get_user(None, 1..).await;
    ctx.mock_get_user_settings(None, 1..).await;
    ctx.mock_get_addresses(Some(vec![test_api_address()]), 1..)
        .await;
}

#[tokio::test]
async fn test_on_refresh_impl_none() {
    // Setup
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    // Test Refresh::None
    let result = refresh(&user_ctx, Refresh::None).await;

    // Should succeed and do nothing
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_on_refresh_impl_unknown() {
    // Setup
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    // Test Refresh::Unknown
    let result = refresh(&user_ctx, Refresh::Unknown(42)).await;

    // Should succeed and do nothing (just log a warning)
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_on_refresh_impl_mail_success() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;
    setup_mail_refresh_mocks(&ctx).await;

    let result = refresh(&user_ctx, Refresh::Mail).await;

    // Should succeed
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_on_refresh_impl_contacts_success() {
    // Setup
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;
    setup_contacts_refresh_mocks(&ctx, 1).await;

    // Test Refresh::Contacts
    let result = refresh(&user_ctx, Refresh::Contacts).await;

    // Should succeed
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_on_refresh_impl_contacts_network_error() {
    // Setup
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;
    setup_contacts_refresh_mocks(&ctx, 0..).await;
    ctx.mock_ping_success().await;

    ctx.mock_get_contacts_respond_with(|mock| {
        mock.respond_with(
            ResponseTemplate::new(500)
                .set_body_json(create_error_response(500, "Internal server error")),
        )
        .with_priority(1)
    })
    .await;

    // Test Refresh::Contacts with network error
    let error = refresh(&user_ctx, Refresh::Contacts).await.unwrap_err();
    let err = error.as_action_error::<ActionRefresh>().unwrap();

    assert!(matches!(
        err,
        ActionError::Action(ActionEventLoopError::EventLoop(EventLoopError::Subscriber(
            _,
            _
        )))
    ));
}

#[tokio::test]
async fn test_on_refresh_impl_retry_behavior() {
    // Setup
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;

    setup_mail_refresh_mocks(&ctx).await;
    // Mock API to fail initially, then succeed on retry
    Mock::given(method("GET"))
        .and(path("/api/mail/v4/settings"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(create_error_response(500, "Temporary server error")),
        )
        .up_to_n_times(4) // 4 requests from Muon as in 3 retries + 1 initial request
        .with_priority(1)
        .mount(ctx.mock_server())
        .await;

    ctx.mock_ping_success().await;

    // Test Refresh::Mail - the internal retry logic will be tested
    let result = refresh(&user_ctx, Refresh::Mail).await;

    // Should succeed on second try
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_on_refresh_impl_different_refresh_types() {
    // Setup
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;
    setup_mail_refresh_mocks(&ctx).await;
    setup_core_refresh_mocks(&ctx).await;

    // Test different refresh types individually
    let result_none = refresh(&user_ctx, Refresh::None).await;
    let result_unknown = refresh(&user_ctx, Refresh::Unknown(42)).await;
    let result_mail = refresh(&user_ctx, Refresh::Mail).await;
    let result_contacts = refresh(&user_ctx, Refresh::Contacts).await;
    let result_all = refresh(&user_ctx, Refresh::All).await;

    assert!(result_none.is_ok());
    assert!(result_unknown.is_ok());
    assert!(result_mail.is_ok());
    assert!(result_contacts.is_ok());
    assert!(result_all.is_ok());
}

#[tokio::test]
async fn test_on_refresh_impl_mail_success_and_refresh_conversations() {
    let ctx = MailTestContext::new().await;
    let user_ctx = set_up_test_conversations(&ctx).await;
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let result = refresh(&user_ctx, Refresh::Mail).await;
    // Should succeed
    assert!(result.is_ok());
    // Check that the conversations from api are saved, and local are deleted
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 2);
    assert!(
        Conversation::find_by_remote_id("myconv_100".into(), &tether)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        Conversation::find_by_remote_id("new_api_conv".into(), &tether)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn test_on_refresh_impl_mail_success_and_refresh_messages_after_mail_settings_update() {
    let ctx = MailTestContext::new().await;
    let user_ctx = set_up_test_messages(&ctx).await;
    let tether = user_ctx.user_stash().connection().await.unwrap();
    let result = refresh(&user_ctx, Refresh::Mail).await;
    // Should succeed
    assert!(result.is_ok());
    // Check that the messages from api are saved, and local are deleted
    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 2);
    // New conversation appeared, because the messages came without conversation id
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 2);

    let msg = Message::find_by_remote_id("mymsg_100".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.remote_conversation_id, Some("".into()));
    let msg = Message::find_by_remote_id("new_api_msg".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.remote_conversation_id, Some("".into()));
}

#[tokio::test]
async fn test_on_refresh_impl_all_success_and_refresh_messages_after_mail_settings_update() {
    let ctx = MailTestContext::new().await;
    let user_ctx = set_up_test_messages(&ctx).await;
    setup_core_refresh_mocks(&ctx).await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    refresh(&user_ctx, Refresh::All).await.unwrap();
    // Check that the messages from api are saved, and local are deleted
    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 2);
    assert!(
        Message::find_by_remote_id("mymsg_100".into(), &tether)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        Message::find_by_remote_id("new_api_msg".into(), &tether)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn test_on_refresh_leaves_messages_without_remote_id_untouched() {
    let ctx = MailTestContext::new().await;
    let user_ctx = set_up_test_messages(&ctx).await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Modify one of the local messages to have no remote id
    let mut local_msg = Message::find_by_remote_id("mymsg_1".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    local_msg.remote_id = None;

    tether
        .tx(async |tx| local_msg.save(tx).await)
        .await
        .unwrap();

    assert!(local_msg.remote_id.is_none());

    let result = refresh(&user_ctx, Refresh::Mail).await;
    // Should succeed
    assert!(result.is_ok());
    // Check that the messages from api are saved, and local are deleted
    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 3);
    // New conversation appeared, because the messages came without conversation id
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 2);

    let msg = Message::find_by_remote_id("mymsg_100".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.remote_conversation_id, Some("".into()));
    let msg = Message::find_by_remote_id("new_api_msg".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.remote_conversation_id, Some("".into()));
    // Local message load
    let msg = Message::load(local_msg.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    // This message was not refreshed, so it still should have the
    // unique conversation id from a set up in the test
    assert_eq!(msg.remote_conversation_id, Some(UNIQUE_CONV_ID.into()));
}

#[tokio::test]
async fn test_on_refresh_leaves_local_draft_messages_untouched() {
    let ctx = MailTestContext::new().await;
    let user_ctx = set_up_test_messages(&ctx).await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();

    // Modify one of the local messages to be a draft
    let msg = Message::find_by_remote_id("mymsg_1".into(), &tether)
        .await
        .unwrap()
        .unwrap();

    tether
        .tx(async |tx| {
            let mut draft = DraftMetadata::empty(tx).await.unwrap();
            draft.local_message_id = msg.local_id;
            draft.local_conversation_id = msg.local_conversation_id;
            draft.save(tx).await
        })
        .await
        .unwrap();

    assert!(msg.is_local_draft(&tether).await.unwrap());

    let result = refresh(&user_ctx, Refresh::Mail).await;
    // Should succeed
    assert!(result.is_ok());
    // Check that the messages from api are saved, and local are deleted
    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 3);
    // New conversation appeared, because the messages came without conversation id
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 2);

    let msg = Message::find_by_remote_id("mymsg_100".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.remote_conversation_id, Some("".into()));
    let msg = Message::find_by_remote_id("new_api_msg".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg.remote_conversation_id, Some("".into()));
    let msg = Message::find_by_remote_id("mymsg_1".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    // This message was not refreshed, so it still should have the
    // unique conversation id from a set up in the test
    assert_eq!(msg.remote_conversation_id, Some(UNIQUE_CONV_ID.into()));
}

#[tokio::test]
async fn test_on_refresh_leaves_local_draft_messages_in_converstation_untouched() {
    let ctx = MailTestContext::new().await;
    let user_ctx = set_up_test_conversations(&ctx).await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let conv = Conversation::find_by_remote_id("myconv_100".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    let draft_msg = create_single_message(&conv, "draft", &mut tether).await;
    let remote_msg = create_single_message(&conv, "remote", &mut tether).await;
    let local_msg = create_single_message(&conv, "local", &mut tether).await;
    let mut no_remote_id_msg = create_single_message(&conv, "no_remote_id", &mut tether).await;
    no_remote_id_msg.remote_id = None;

    tether
        .tx(async |tx| {
            no_remote_id_msg.save(tx).await?;
            let mut draft = DraftMetadata::empty(tx).await.unwrap();
            draft.local_message_id = draft_msg.local_id;
            draft.local_conversation_id = draft_msg.local_conversation_id;
            draft.save(tx).await
        })
        .await
        .unwrap();

    assert!(draft_msg.is_local_draft(&tether).await.unwrap());
    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 4);

    ctx.mock_get_message_metadata_and(
        vec![api_message_meta!(
                id: remote_msg.remote_id.clone().unwrap(),
                address_id: MY_ADDRESS_ID.clone(),
                conversation_id: conv.remote_id.unwrap())],
        |mock| mock.with_priority(1).up_to_n_times(1),
        1,
    )
    .await;

    let result = refresh(&user_ctx, Refresh::Mail).await;
    // Should succeed
    assert!(result.is_ok());
    // Check that the messages from api are saved, and local are deleted
    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 3);
    // `myconv_100` & `new_api_conv`
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 2);

    assert!(draft_msg.exists(&tether).await.unwrap());
    assert!(remote_msg.exists(&tether).await.unwrap());
    assert!(no_remote_id_msg.exists(&tether).await.unwrap());
    // Should be deleted
    assert!(!local_msg.exists(&tether).await.unwrap());
}

#[tokio::test]
async fn test_on_refresh_leaves_conversation_without_remote_id_untouched() {
    let ctx = MailTestContext::new().await;
    let user_ctx = set_up_test_conversations(&ctx).await;
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    let mut local_conv = Conversation::find_by_remote_id("myconv_1".into(), &tether)
        .await
        .unwrap()
        .unwrap();
    local_conv.remote_id = None;
    tether
        .tx(async |tx| local_conv.save(tx).await)
        .await
        .unwrap();
    assert!(local_conv.remote_id.is_none());

    let result = refresh(&user_ctx, Refresh::Mail).await;
    // Should succeed
    assert!(result.is_ok());
    // Check that the conversations from api are saved, and local are deleted
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 3);
    assert!(
        Conversation::find_by_remote_id("myconv_100".into(), &tether)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        Conversation::find_by_remote_id("new_api_conv".into(), &tether)
            .await
            .unwrap()
            .is_some()
    );
    // Shouldn't be deleted
    assert!(local_conv.exists(&tether).await.unwrap());
}

async fn set_up_test_messages(ctx: &MailTestContext) -> Arc<MailUserContext> {
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "1")), 1..)
        .await;
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "3")), 1..)
        .await;
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "4")), 1..)
        .await;
    ctx.mock_get_conversations_count(None, 1..).await;
    ctx.mock_get_messages_count(None, 1..).await;
    let mut mail_settings = DEFAULT_MAIL_SETTINGS.clone();
    mail_settings.view_mode = ViewMode::Messages;
    ctx.mock_get_mail_settings(Some(mail_settings), 1..).await;
    ctx.mock_ping_success().await;
    ctx.mock_get_incoming_defaults(None, 1..).await;
    ctx.mock_get_message_metadata_and(
        vec![
            api_message_meta!(id: "mymsg_100".into(), address_id: MY_ADDRESS_ID.clone()),
            api_message_meta!(id: "new_api_msg".into(), address_id: MY_ADDRESS_ID.clone()),
        ],
        |mock| mock.with_priority(1).up_to_n_times(1),
        1,
    )
    .await;
    ctx.mock_get_message_metadata(vec![], 1..).await;

    let mut data = hash_map!(
        vec![SystemLabel::Inbox.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_1".into()]: test_messages(10, 0),
        vec![SystemLabel::Sent.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_2".into()]: test_messages(100, 10),
    );
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    data.save_to_database(&mut tether).await;

    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 110);
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 1);

    user_ctx
}

async fn set_up_test_conversations(ctx: &MailTestContext) -> Arc<MailUserContext> {
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;
    setup_mail_refresh_mocks(ctx).await;
    ctx.mock_ping_success().await;
    ctx.mock_get_conversations_and(
        vec![
            api_conversation!(id: "myconv_100".into()),
            api_conversation!(id: "new_api_conv".into()),
        ],
        |mock| mock.with_priority(1).up_to_n_times(1),
        1,
    )
    .await;
    ctx.mock_get_conversations(vec![], 1..).await;

    let mut data = hash_map!(
        vec![SystemLabel::Inbox.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_1".into()]:
            test_conversations(10, 0),
        vec![SystemLabel::Sent.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_2".into()]:
            test_conversations(100, 10),
    );
    let mut tether = user_ctx.user_stash().connection().await.unwrap();
    data.save_to_database(&mut tether).await;

    user_ctx
}
