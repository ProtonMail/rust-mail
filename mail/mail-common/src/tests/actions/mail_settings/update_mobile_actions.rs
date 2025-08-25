use crate::actions::mail_settings::{ToolbarType, UpdateMobileActions};
use crate::datatypes::MobileAction;
use test_case::test_case;

#[test_case(ToolbarType::List, vec![MobileAction::ToggleRead, MobileAction::Trash, MobileAction::Move] ; "valid list actions")]
#[test_case(ToolbarType::Message, vec![MobileAction::Reply, MobileAction::Forward, MobileAction::Print] ; "valid message actions")]
#[test_case(ToolbarType::Conversation, vec![MobileAction::ToggleRead, MobileAction::Archive, MobileAction::Spam] ; "valid conversation actions")]
#[test_case(ToolbarType::List, vec![MobileAction::ToggleRead] ; "single valid action")]
#[test_case(ToolbarType::Message, vec![MobileAction::ToggleRead, MobileAction::ToggleStar, MobileAction::Archive, MobileAction::Trash, MobileAction::Reply] ; "max five actions")]
fn test_update_mobile_actions_valid(toolbar_type: ToolbarType, actions: Vec<MobileAction>) {
    let action_result = UpdateMobileActions::new(toolbar_type, actions, false);
    assert!(
        action_result.is_ok(),
        "Expected valid action creation to succeed"
    );
}

#[test_case(ToolbarType::List, vec![MobileAction::ToggleRead, MobileAction::Trash, MobileAction::Move, MobileAction::Archive, MobileAction::Spam, MobileAction::Label] ; "too many actions")]
#[test_case(ToolbarType::Message, vec![MobileAction::Snooze] ; "invalid action for message toolbar")]
fn test_update_mobile_actions_invalid(toolbar_type: ToolbarType, actions: Vec<MobileAction>) {
    let action_result = UpdateMobileActions::new(toolbar_type, actions, false);
    assert!(
        action_result.is_err(),
        "Expected invalid action creation to fail"
    );
}

#[test]
fn test_list_toolbar_validation() {
    let all_list_actions = MobileAction::all_list_actions();

    // Test that each individual action is valid for list toolbar (not all together due to 5-action limit)
    for action in &all_list_actions {
        let result = UpdateMobileActions::new(ToolbarType::List, vec![action.clone()], false);
        assert!(
            result.is_ok(),
            "List action {action:?} should be valid for list toolbar"
        );
    }

    // Test that a subset (within 5-action limit) is valid
    let subset = all_list_actions.into_iter().take(5).collect();
    let result = UpdateMobileActions::new(ToolbarType::List, subset, false);
    assert!(result.is_ok(), "Subset of list actions should be valid");
}

#[test]
fn test_message_toolbar_validation() {
    let all_message_actions = MobileAction::all_message_actions();

    // Test that each individual action is valid for message toolbar
    for action in &all_message_actions {
        let result = UpdateMobileActions::new(ToolbarType::Message, vec![action.clone()], false);
        assert!(
            result.is_ok(),
            "Message action {action:?} should be valid for message toolbar"
        );
    }

    // Test that a subset (within 5-action limit) is valid
    let subset = all_message_actions.into_iter().take(5).collect();
    let result = UpdateMobileActions::new(ToolbarType::Message, subset, false);
    assert!(result.is_ok(), "Subset of message actions should be valid");
}

#[test]
fn test_conversation_toolbar_validation() {
    let all_conversation_actions = MobileAction::all_conversation_actions();

    // Test that each individual action is valid for conversation toolbar
    for action in &all_conversation_actions {
        let result =
            UpdateMobileActions::new(ToolbarType::Conversation, vec![action.clone()], false);
        assert!(
            result.is_ok(),
            "Conversation action {action:?} should be valid for conversation toolbar"
        );
    }

    // Test that a subset (within 5-action limit) is valid
    let subset = all_conversation_actions.into_iter().take(5).collect();
    let result = UpdateMobileActions::new(ToolbarType::Conversation, subset, false);
    assert!(
        result.is_ok(),
        "Subset of conversation actions should be valid"
    );
}

#[test]
fn test_maximum_actions_limit() {
    let actions = vec![
        MobileAction::ToggleRead,
        MobileAction::ToggleStar,
        MobileAction::Archive,
        MobileAction::Trash,
        MobileAction::Move,
        MobileAction::Label, // 6th action - should fail
    ];
    let result = UpdateMobileActions::new(ToolbarType::List, actions, false);
    assert!(result.is_err(), "More than 5 actions should be rejected");
}

#[test]
fn test_empty_actions_allowed() {
    let result = UpdateMobileActions::new(ToolbarType::List, vec![], false);
    assert!(result.is_ok(), "Empty actions should be allowed");
}

#[test]
fn test_duplicate_actions_allowed() {
    let actions = vec![MobileAction::ToggleRead, MobileAction::ToggleRead];
    let result = UpdateMobileActions::new(ToolbarType::List, actions, false);
    assert!(
        result.is_ok(),
        "Duplicate actions should be verified on the client side"
    );
}

#[test]
fn test_default_mobile_actions_integration() {
    let default_list_actions = MobileAction::default_chosen_actions();
    let result = UpdateMobileActions::new(ToolbarType::List, default_list_actions.clone(), false);
    assert!(
        result.is_ok(),
        "Should be able to create action for any toolbar type"
    );
    let result =
        UpdateMobileActions::new(ToolbarType::Message, default_list_actions.clone(), false);
    assert!(
        result.is_ok(),
        "Should be able to create action for any toolbar type"
    );
    let result = UpdateMobileActions::new(ToolbarType::Conversation, default_list_actions, false);
    assert!(
        result.is_ok(),
        "Should be able to create action for any toolbar type"
    );
}
