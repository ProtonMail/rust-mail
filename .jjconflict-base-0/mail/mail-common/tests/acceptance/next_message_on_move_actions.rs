use proton_mail_api::services::proton::request_data::PutNextMessageOnMoveRequest;
use proton_mail_api::services::proton::responses::PutNextMessageOnMoveResponse;
use proton_mail_common::datatypes::NextMessageOnMove;
use proton_mail_common::models::MailSettings;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use wiremock::ResponseTemplate;

fn test_init_params() -> TestParams {
    TestParams {
        ..Default::default()
    }
}

fn success_response_enabled() -> ResponseTemplate {
    let mail_settings = proton_mail_api::services::proton::response_data::MailSettings {
        next_message_on_move: Some(
            proton_mail_api::services::proton::response_data::NextMessageOnMove::EnabledExplicit,
        ),
        ..Default::default()
    };

    ResponseTemplate::new(200).set_body_json(PutNextMessageOnMoveResponse {
        code: 1000,
        mail_settings,
    })
}

fn success_response_disabled() -> ResponseTemplate {
    let mail_settings = proton_mail_api::services::proton::response_data::MailSettings {
        next_message_on_move: Some(
            proton_mail_api::services::proton::response_data::NextMessageOnMove::DisabledExplicit,
        ),
        ..Default::default()
    };

    ResponseTemplate::new(200).set_body_json(PutNextMessageOnMoveResponse {
        code: 1000,
        mail_settings,
    })
}

fn error_response() -> ResponseTemplate {
    ResponseTemplate::new(500).set_body_json(serde_json::json!({
        "error": "Internal server error"
    }))
}

#[tokio::test]
async fn test_enable_next_message_on_move() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let params = test_init_params();

    ctx.setup_user(params.clone()).await;

    let expected_request = PutNextMessageOnMoveRequest {
        next_message_on_move: true,
    };

    ctx.mock_put_next_message_on_move(success_response_enabled(), expected_request, 1)
        .await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let initial_settings = MailSettings::get_or_default(&tether).await;
    assert_eq!(
        initial_settings.next_message_on_move, None,
        "Should start with no next message on move setting"
    );

    MailSettings::action_update_next_message_on_move(user_ctx.action_queue(), true)
        .await
        .expect("Should successfully enqueue action");

    user_ctx.execute_single_action().await.unwrap();

    let updated_settings = MailSettings::get_or_default(&tether).await;
    assert_eq!(
        updated_settings.next_message_on_move,
        Some(NextMessageOnMove::EnabledExplicit),
        "Next message on move should be enabled after action execution"
    );
}

#[tokio::test]
async fn test_disable_next_message_on_move() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let params = test_init_params();

    ctx.setup_user(params.clone()).await;

    let expected_request = PutNextMessageOnMoveRequest {
        next_message_on_move: false,
    };

    ctx.mock_put_next_message_on_move(success_response_disabled(), expected_request, 1)
        .await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let initial_settings = MailSettings::get_or_default(&tether).await;
    assert_eq!(
        initial_settings.next_message_on_move, None,
        "Should start with no next message on move setting"
    );

    MailSettings::action_update_next_message_on_move(user_ctx.action_queue(), false)
        .await
        .expect("Should successfully enqueue action");

    user_ctx.execute_single_action().await.unwrap();

    let updated_settings = MailSettings::get_or_default(&tether).await;
    assert_eq!(
        updated_settings.next_message_on_move,
        Some(NextMessageOnMove::DisabledExplicit),
        "Next message on move should be disabled after action execution"
    );
}

#[tokio::test]
async fn test_next_message_on_move_api_failure() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let params = test_init_params();
    ctx.setup_user(params.clone()).await;

    let expected_request = PutNextMessageOnMoveRequest {
        next_message_on_move: true,
    };

    ctx.mock_put_next_message_on_move(error_response(), expected_request, 4)
        .await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    MailSettings::action_update_next_message_on_move(user_ctx.action_queue(), true)
        .await
        .expect("Should successfully enqueue action");

    let result = user_ctx.execute_single_action().await;
    assert!(result.is_err(), "Action should fail when API fails");

    let settings_after_failure = MailSettings::get_or_default(&tether).await;
    assert_eq!(
        settings_after_failure.next_message_on_move, None,
        "Local setting should be reverted when API fails"
    );
}
