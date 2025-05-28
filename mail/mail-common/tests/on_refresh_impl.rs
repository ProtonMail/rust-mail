use proton_core_common::datatypes::{Refresh, SystemLabel};
use proton_core_common::models::ModelIdExtension;
use proton_core_common::test_utils::addresses::MY_ADDRESS_ID;
use proton_event_loop::subscriber::SubscriberError;
use proton_mail_api::services::proton::prelude::ViewMode;
use proton_mail_common::models::{Conversation, Message};
use proton_mail_common::test_utils::init::{DEFAULT_MAIL_SETTINGS, Params as TestParams};
use proton_mail_common::test_utils::scroller::{
    StoreLabeledModelMap, test_conversations, test_messages,
};
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use proton_mail_common::test_utils::utils::test_api_address;
use proton_mail_common::{api_conversation, api_message_meta};
use stash::orm::Model;
use velcro::hash_map;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate, Times};

fn create_error_response(code: u16, message: &str) -> serde_json::Value {
    serde_json::json!({
        "Code": code,
        "Error": message
    })
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
}

async fn setup_contacts_refresh_mocks(ctx: &MailTestContext, expect: impl Into<Times> + Clone) {
    ctx.mock_get_contacts(None, expect.clone().into()).await;
    ctx.mock_get_contacts_emails(None, expect.into()).await;
}

async fn setup_core_refresh_mocks(ctx: &MailTestContext) {
    setup_contacts_refresh_mocks(ctx, 1..).await;
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "2")), 1..)
        .await;
    ctx.mock_get_user(None, 1..).await;
    ctx.mock_user_settings(None, 1..).await;
    ctx.mock_get_addresses(None, 1..).await;
}

#[tokio::test]
async fn test_on_refresh_impl_none() {
    // Setup
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    // Test Refresh::None
    let result = user_ctx.on_refresh_impl(Refresh::None).await;

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
    let result = user_ctx.on_refresh_impl(Refresh::Unknown(42)).await;

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
    ctx.catch_all().await;

    let result = user_ctx.on_refresh_impl(Refresh::Mail).await;

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
    ctx.catch_all().await;

    // Test Refresh::Contacts
    let result = user_ctx.on_refresh_impl(Refresh::Contacts).await;

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

    // Mock API to return network error for contacts endpoint
    Mock::given(method("GET"))
        .and(path("/api/contacts"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_json(create_error_response(500, "Internal server error")),
        )
        .with_priority(1)
        .mount(ctx.mock_server())
        .await;

    ctx.catch_all().await;

    // Test Refresh::Contacts with network error
    let result = user_ctx.on_refresh_impl(Refresh::Contacts).await;

    // Should fail with SubscriberError
    assert!(result.is_err());
    match result.unwrap_err() {
        SubscriberError::Api(_) => {
            // Expected API error
        }
        SubscriberError::Other(_) => {
            // Also acceptable as it might be wrapped
        }
        other => panic!("Unexpected error type: {:?}", other),
    }
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

    ctx.catch_all().await;

    // Test Refresh::Mail - the internal retry logic will be tested
    let result = user_ctx.on_refresh_impl(Refresh::Mail).await;

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
    ctx.catch_all().await;

    // Test different refresh types individually
    let result_none = user_ctx.on_refresh_impl(Refresh::None).await;
    let result_unknown = user_ctx.on_refresh_impl(Refresh::Unknown(42)).await;
    let result_mail = user_ctx.on_refresh_impl(Refresh::Mail).await;
    let result_contacts = user_ctx.on_refresh_impl(Refresh::Contacts).await;
    let result_all = user_ctx.on_refresh_impl(Refresh::All).await;

    assert!(result_none.is_ok());
    assert!(result_unknown.is_ok());
    assert!(result_mail.is_ok());
    assert!(result_contacts.is_ok());
    assert!(result_all.is_ok());
}

#[tokio::test]
async fn test_on_refresh_impl_mail_success_and_refresh_conversations() {
    let ctx = MailTestContext::new().await;
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;
    setup_mail_refresh_mocks(&ctx).await;
    ctx.mock_ping_success().await;
    ctx.mock_get_incoming_defaults(None, 1..).await;
    ctx.mock_get_conversations_and(
        vec![
            api_conversation!(id: "myconv_110".into()),
            api_conversation!(id: "new_api_conv".into()),
        ],
        |mock| mock.with_priority(1).up_to_n_times(1),
        1,
    )
    .await;
    ctx.mock_get_conversations(vec![], 1..).await;
    ctx.catch_all().await;

    let mut data = hash_map!(
        vec![SystemLabel::Inbox.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_1".into()]: test_conversations(10, 0),
        vec![SystemLabel::Sent.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_2".into()]: test_conversations(100, 10),
    );
    let mut tether = user_ctx.user_stash().connection();
    data.save_to_database(&mut tether).await;

    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 110);

    let result = user_ctx.on_refresh_impl(Refresh::Mail).await;
    // Should succeed
    assert!(result.is_ok());
    // Execute all actions - 3 conversation actions, 1 incoming defaults action
    user_ctx.execute_all_actions().await.unwrap();
    // Check that the conversations from api are saved, and local are deleted
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 2);
    assert!(
        Conversation::find_by_remote_id("myconv_110".into(), &tether)
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
            api_message_meta!(id: "mymsg_110".into(), address_id: MY_ADDRESS_ID.clone()),
            api_message_meta!(id: "new_api_msg".into(), address_id: MY_ADDRESS_ID.clone()),
        ],
        |mock| mock.with_priority(1).up_to_n_times(1),
        1,
    )
    .await;
    ctx.mock_get_message_metadata(vec![], 1..).await;
    ctx.catch_all().await;

    let mut data = hash_map!(
        vec![SystemLabel::Inbox.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_1".into()]: test_messages(10, 0),
        vec![SystemLabel::Sent.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_2".into()]: test_messages(100, 10),
    );
    let mut tether = user_ctx.user_stash().connection();
    data.save_to_database(&mut tether).await;

    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 110);
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 1);

    let result = user_ctx.on_refresh_impl(Refresh::Mail).await;
    // Should succeed
    assert!(result.is_ok());
    // Execute all actions - 3 message actions, 1 incoming defaults action
    user_ctx.execute_all_actions().await.unwrap();
    // Check that the messages from api are saved, and local are deleted
    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 2);
    // New conversation appeared, because the messages came without conversation id
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 2);

    let msg = Message::find_by_remote_id("mymsg_110".into(), &tether)
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
    let params = TestParams::default_basic();
    ctx.setup_user(params).await;
    let user_ctx = ctx.mail_user_context().await;

    ctx.mock_server().reset().await;
    setup_contacts_refresh_mocks(&ctx, 1..).await;
    ctx.mock_get_labels_and(vec![], |mock| mock.and(query_param("Type", "2")), 1..)
        .await;
    ctx.mock_get_user(None, 1..).await;
    ctx.mock_user_settings(None, 1..).await;
    ctx.mock_get_addresses(Some(vec![test_api_address()]), 1..)
        .await;
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
            api_message_meta!(id: "mymsg_110".into(), address_id: MY_ADDRESS_ID.clone()),
            api_message_meta!(id: "new_api_msg".into(), address_id: MY_ADDRESS_ID.clone()),
        ],
        |mock| mock.with_priority(1).up_to_n_times(1),
        1,
    )
    .await;
    ctx.mock_get_message_metadata(vec![], 1..).await;
    ctx.catch_all().await;

    let mut data = hash_map!(
        vec![SystemLabel::Inbox.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_1".into()]: test_messages(10, 0),
        vec![SystemLabel::Sent.remote_id(), SystemLabel::AllMail.remote_id(), "mylabel_2".into()]: test_messages(100, 10),
    );
    let mut tether = user_ctx.user_stash().connection();
    data.save_to_database(&mut tether).await;

    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 110);
    assert_eq!(Conversation::count("", vec![], &tether).await.unwrap(), 1);

    let result = user_ctx.on_refresh_impl(Refresh::All).await;
    // Should succeed
    assert!(result.is_ok());
    // Execute all actions - 3 message actions, 1 incoming defaults action
    user_ctx.execute_all_actions().await.unwrap();
    // Check that the messages from api are saved, and local are deleted
    assert_eq!(Message::count("", vec![], &tether).await.unwrap(), 2);
    assert!(
        Message::find_by_remote_id("mymsg_110".into(), &tether)
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
