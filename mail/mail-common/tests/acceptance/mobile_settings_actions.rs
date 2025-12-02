use proton_mail_api::services::proton::request_data::PutMobileSettings;
use proton_mail_api::services::proton::response_data::MobileAction as ApiMobileAction;
use proton_mail_api::services::proton::responses::PutMobileSettingsResponse;
use proton_mail_common::datatypes::MobileAction;
use proton_mail_common::models::MailSettings;
use proton_mail_common::test_utils::init::Params as TestParams;
use proton_mail_common::test_utils::test_context::{MailTestContext, MailUserContextTestExtension};
use wiremock::ResponseTemplate;

fn test_init_params() -> TestParams {
    TestParams {
        ..Default::default()
    }
}

fn success_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(PutMobileSettingsResponse { code: 1000 })
}

fn error_response() -> ResponseTemplate {
    ResponseTemplate::new(500).set_body_json(serde_json::json!({
        "error": "Internal server error"
    }))
}

/// Validates that mobile toolbar actions can be updated via the action queue system.
/// This is a comprehensive acceptance test that follows the full action execution flow:
/// 1. Sets up a user context with mocked APIs
/// 2. Enqueues an UpdateMobileActions action via convenience method
/// 3. Executes the action through the queue
/// 4. Verifies the action updates the local database correctly
/// 5. Verifies the action syncs with the API (mocked)
#[tokio::test]
async fn test_update_list_toolbar_actions() {
    // General setup
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let params = test_init_params();

    // Set up mocks
    ctx.setup_user(params.clone()).await;

    // Mock the mobile settings API endpoint
    let expected_put_mobile_settings = PutMobileSettings {
        list_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::ToggleStar,
            ApiMobileAction::Archive,
            ApiMobileAction::Trash,
        ],
        message_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::Trash,
            ApiMobileAction::Move,
            ApiMobileAction::Label,
        ],
        conversation_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::Trash,
            ApiMobileAction::Move,
            ApiMobileAction::Label,
        ],
    };

    ctx.mock_put_mobile_settings(success_response(), expected_put_mobile_settings, 1)
        .await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Get initial mail settings to verify baseline
    let initial_settings = MailSettings::get_or_default(&tether).await;
    assert!(
        initial_settings.mobile_settings.is_none(),
        "Should start with no mobile settings"
    );

    // Test: Update list toolbar actions using convenience method
    let list_actions = vec![
        MobileAction::ToggleRead,
        MobileAction::ToggleStar,
        MobileAction::Archive,
        MobileAction::Trash,
    ];

    // Enqueue the action using the convenience method
    MailSettings::action_update_list_toolbar(user_ctx.action_queue(), list_actions.clone(), false)
        .await
        .expect("Should successfully enqueue action");

    // Execute the action through the queue
    user_ctx.execute_single_action().await.unwrap();

    // Verify the action updated the local database
    let updated_settings = MailSettings::get_or_default(&tether).await;

    assert!(
        updated_settings.mobile_settings.is_some(),
        "Mobile settings should be set"
    );
    let mobile_settings = updated_settings.mobile_settings.unwrap();

    // Verify is_custom was set to true for list toolbar
    assert!(
        mobile_settings.list_toolbar.is_custom,
        "List toolbar should be marked as custom"
    );

    // Verify list toolbar actions were updated
    let stored_actions = &mobile_settings.list_toolbar.actions;
    assert_eq!(stored_actions.len(), 4, "Should have 4 list actions");

    // Actions are now stored as enums directly, no conversion needed
    let stored_mobile_actions = stored_actions.clone();

    assert_eq!(
        stored_mobile_actions, list_actions,
        "Stored actions should match input actions"
    );

    // Verify other toolbars remain unchanged (default)
    assert!(
        !mobile_settings.message_toolbar.is_custom,
        "Message toolbar should remain default"
    );
    assert!(
        !mobile_settings.conversation_toolbar.is_custom,
        "Conversation toolbar should remain default"
    );
}

/// Test updating message toolbar actions specifically
#[tokio::test]
async fn test_update_message_toolbar_actions() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let params = test_init_params();
    ctx.setup_user(params.clone()).await;

    let message_actions = vec![
        MobileAction::Reply,
        MobileAction::Forward,
        MobileAction::Print,
    ];

    // Mock the API call for message toolbar update
    let expected_put_mobile_settings = PutMobileSettings {
        list_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::Trash,
            ApiMobileAction::Move,
            ApiMobileAction::Label,
        ],
        message_toolbar: vec![
            ApiMobileAction::Reply,
            ApiMobileAction::Forward,
            ApiMobileAction::Print,
        ],
        conversation_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::Trash,
            ApiMobileAction::Move,
            ApiMobileAction::Label,
        ],
    };

    ctx.mock_put_mobile_settings(success_response(), expected_put_mobile_settings, 1)
        .await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Enqueue and execute the action
    MailSettings::action_update_message_toolbar(
        user_ctx.action_queue(),
        message_actions.clone(),
        false,
    )
    .await
    .expect("Should successfully enqueue action");

    user_ctx.execute_single_action().await.unwrap();

    // Verify the action updated the database correctly
    let updated_settings = MailSettings::get_or_default(&tether).await;
    let mobile_settings = updated_settings.mobile_settings.unwrap();

    assert!(
        mobile_settings.message_toolbar.is_custom,
        "Message toolbar should be marked as custom"
    );

    let stored_actions = &mobile_settings.message_toolbar.actions;
    assert_eq!(stored_actions.len(), 3, "Should have 3 message actions");
    assert!(stored_actions.contains(&MobileAction::Reply));
    assert!(stored_actions.contains(&MobileAction::Forward));
    assert!(stored_actions.contains(&MobileAction::Print));

    // Verify other toolbars remain default
    assert!(
        !mobile_settings.list_toolbar.is_custom,
        "List toolbar should remain default"
    );
    assert!(
        !mobile_settings.conversation_toolbar.is_custom,
        "Conversation toolbar should remain default"
    );
}

/// Tests updating conversation toolbar actions through the mobile actions system.
/// This test validates conversation-specific actions and API interaction.
#[tokio::test]
async fn test_update_conversation_toolbar_actions() {
    // General setup
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let params = test_init_params();

    // Set up mocks
    ctx.setup_user(params.clone()).await;

    let conversation_actions = vec![
        MobileAction::ToggleRead,
        MobileAction::ToggleStar,
        MobileAction::Archive,
        MobileAction::Label,
        MobileAction::Move,
    ];

    // Mock the API call for conversation toolbar update
    let expected_put_mobile_settings = PutMobileSettings {
        list_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::Trash,
            ApiMobileAction::Move,
            ApiMobileAction::Label,
        ],
        message_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::Trash,
            ApiMobileAction::Move,
            ApiMobileAction::Label,
        ],
        conversation_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::ToggleStar,
            ApiMobileAction::Archive,
            ApiMobileAction::Label,
            ApiMobileAction::Move,
        ],
    };

    ctx.mock_put_mobile_settings(success_response(), expected_put_mobile_settings, 1)
        .await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    // Execute: Update conversation toolbar actions
    MailSettings::action_update_conversation_toolbar(
        user_ctx.action_queue(),
        conversation_actions.clone(),
        false,
    )
    .await
    .expect("Should queue conversation toolbar action");

    // Execute the action and verify success
    let result = user_ctx.execute_single_action().await;
    assert!(result.is_ok(), "Action should succeed: {:?}", result.err());

    // Verify the action updated the local database correctly
    let updated_settings = MailSettings::get_or_default(&tether).await;
    let mobile_settings = updated_settings.mobile_settings.unwrap();

    assert!(
        mobile_settings.conversation_toolbar.is_custom,
        "Conversation toolbar should be marked as custom"
    );

    let stored_actions = &mobile_settings.conversation_toolbar.actions;
    assert_eq!(
        stored_actions.len(),
        5,
        "Should have 5 conversation actions"
    );
    assert!(stored_actions.contains(&MobileAction::ToggleRead));
    assert!(stored_actions.contains(&MobileAction::ToggleStar));
    assert!(stored_actions.contains(&MobileAction::Archive));
    assert!(stored_actions.contains(&MobileAction::Label));
    assert!(stored_actions.contains(&MobileAction::Move));

    // Verify other toolbars remain default
    assert!(
        !mobile_settings.list_toolbar.is_custom,
        "List toolbar should remain default"
    );
    assert!(
        !mobile_settings.message_toolbar.is_custom,
        "Message toolbar should remain default"
    );
}

/// Test error handling when API call fails
#[tokio::test]
async fn test_api_failure_handling() {
    let ctx = MailTestContext::new().await;
    let user_ctx = ctx.uninitialized_mail_user_context().await;
    let tether = user_ctx.user_stash().connection().await.unwrap();

    let params = test_init_params();
    ctx.setup_user(params.clone()).await;

    // Mock API failure - action queue retries, so expect multiple calls
    // The mock needs to expect the API payload that would be sent for list toolbar update
    let expected_put_mobile_settings = PutMobileSettings {
        list_toolbar: vec![ApiMobileAction::ToggleRead, ApiMobileAction::Archive],
        message_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::Trash,
            ApiMobileAction::Move,
            ApiMobileAction::Label,
        ],
        conversation_toolbar: vec![
            ApiMobileAction::ToggleRead,
            ApiMobileAction::Trash,
            ApiMobileAction::Move,
            ApiMobileAction::Label,
        ],
    };

    ctx.mock_put_mobile_settings(error_response(), expected_put_mobile_settings, 4)
        .await;

    ctx.initialize_uninitialized_ctx(&user_ctx).await;

    let actions = vec![MobileAction::ToggleRead, MobileAction::Archive];

    // Enqueue the action
    MailSettings::action_update_list_toolbar(user_ctx.action_queue(), actions.clone(), false)
        .await
        .expect("Should successfully enqueue action");

    // Execute the action - should fail during remote sync and revert local changes
    let result = user_ctx.execute_single_action().await;

    // The action should fail during remote execution
    assert!(result.is_err(), "Action should fail due to API error");

    // Local changes should be reverted due to API failure (consistency model)
    let updated_settings = MailSettings::get_or_default(&tether).await;

    // After revert, mobile settings should be back to initial state (empty/default)
    let mobile_settings = updated_settings
        .mobile_settings
        .expect("Mobile settings should exist after revert");

    // Verify that all toolbars are reverted to default state
    assert!(
        !mobile_settings.list_toolbar.is_custom,
        "List toolbar should not be marked as custom after revert"
    );
    assert!(
        !mobile_settings.message_toolbar.is_custom,
        "Message toolbar should not be marked as custom after revert"
    );
    assert!(
        !mobile_settings.conversation_toolbar.is_custom,
        "Conversation toolbar should not be marked as custom after revert"
    );
}
