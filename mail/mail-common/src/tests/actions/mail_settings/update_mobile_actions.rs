use crate::actions::mail_settings::{ToolbarType, UpdateMobileActions};
use crate::datatypes::MobileAction;
use test_case::test_case;

#[test_case(ToolbarType::List, vec![MobileAction::ToggleRead, MobileAction::Trash, MobileAction::Move] ; "valid list actions")]
#[test_case(ToolbarType::Message, vec![MobileAction::Reply, MobileAction::Forward, MobileAction::Print] ; "valid message actions")]
#[test_case(ToolbarType::Conversation, vec![MobileAction::ToggleRead, MobileAction::Archive, MobileAction::Spam] ; "valid conversation actions")]
#[test_case(ToolbarType::List, vec![MobileAction::ToggleRead] ; "single valid action")]
#[test_case(ToolbarType::Message, vec![MobileAction::ToggleRead, MobileAction::ToggleStar, MobileAction::Archive, MobileAction::Trash, MobileAction::Reply] ; "max five actions")]
fn test_update_mobile_actions_valid(toolbar_type: ToolbarType, actions: Vec<MobileAction>) {
    let action_result = UpdateMobileActions::new(toolbar_type, actions);
    assert!(
        action_result.is_ok(),
        "Expected valid action creation to succeed"
    );
}

#[test_case(ToolbarType::List, vec![MobileAction::ToggleRead, MobileAction::Trash, MobileAction::Move, MobileAction::Archive, MobileAction::Spam, MobileAction::Label] ; "too many actions")]
#[test_case(ToolbarType::Message, vec![MobileAction::Snooze] ; "invalid action for message toolbar")]
fn test_update_mobile_actions_invalid(toolbar_type: ToolbarType, actions: Vec<MobileAction>) {
    let action_result = UpdateMobileActions::new(toolbar_type, actions);
    assert!(
        action_result.is_err(),
        "Expected invalid action creation to fail"
    );
}

#[test_case(MobileAction::ToggleRead, "toggle_read")]
#[test_case(MobileAction::ToggleStar, "toggle_star")]
#[test_case(MobileAction::Archive, "archive")]
#[test_case(MobileAction::Trash, "trash")]
#[test_case(MobileAction::Reply, "reply")]
#[test_case(MobileAction::Forward, "forward")]
#[test_case(MobileAction::Print, "print")]
#[test_case(MobileAction::SavePDF, "save_pdf")]
#[test_case(MobileAction::SaveAttachments, "save_attachments")]
#[test_case(MobileAction::ViewHeaders, "view_headers")]
#[test_case(MobileAction::ViewHTML, "view_html")]
#[test_case(MobileAction::Remind, "remind")]
#[test_case(MobileAction::Snooze, "snooze")]
#[test_case(MobileAction::SenderEmails, "sender_emails")]
#[test_case(MobileAction::ReportPhishing, "report_phishing")]
#[test_case(MobileAction::ToggleLight, "toggle_light")]
#[test_case(MobileAction::Move, "move")]
#[test_case(MobileAction::Label, "label")]
#[test_case(MobileAction::Spam, "spam")]
fn test_mobile_action_display(action: MobileAction, expected_string: &str) {
    assert_eq!(
        action.to_string(),
        expected_string,
        "Display implementation should match expected string"
    );

    use std::str::FromStr;
    let parsed_back = MobileAction::from_str(expected_string).expect("Should parse back");
    assert_eq!(
        action, parsed_back,
        "Action should round-trip through string conversion"
    );
    assert_eq!(
        parsed_back.to_string(),
        expected_string,
        "Roundtrip should preserve string representation"
    );
}

#[test]
fn test_list_toolbar_validation() {
    let all_list_actions = MobileAction::all_list_actions();

    // Test that each individual action is valid for list toolbar (not all together due to 5-action limit)
    for action in &all_list_actions {
        let result = UpdateMobileActions::new(ToolbarType::List, vec![action.clone()]);
        assert!(
            result.is_ok(),
            "List action {action:?} should be valid for list toolbar"
        );
    }

    // Test that a subset (within 5-action limit) is valid
    let subset = all_list_actions.into_iter().take(5).collect();
    let result = UpdateMobileActions::new(ToolbarType::List, subset);
    assert!(result.is_ok(), "Subset of list actions should be valid");
}

#[test]
fn test_message_toolbar_validation() {
    let all_message_actions = MobileAction::all_message_actions();

    // Test that each individual action is valid for message toolbar
    for action in &all_message_actions {
        let result = UpdateMobileActions::new(ToolbarType::Message, vec![action.clone()]);
        assert!(
            result.is_ok(),
            "Message action {action:?} should be valid for message toolbar"
        );
    }

    // Test that a subset (within 5-action limit) is valid
    let subset = all_message_actions.into_iter().take(5).collect();
    let result = UpdateMobileActions::new(ToolbarType::Message, subset);
    assert!(result.is_ok(), "Subset of message actions should be valid");
}

#[test]
fn test_conversation_toolbar_validation() {
    let all_conversation_actions = MobileAction::all_conversation_actions();

    // Test that each individual action is valid for conversation toolbar
    for action in &all_conversation_actions {
        let result = UpdateMobileActions::new(ToolbarType::Conversation, vec![action.clone()]);
        assert!(
            result.is_ok(),
            "Conversation action {action:?} should be valid for conversation toolbar"
        );
    }

    // Test that a subset (within 5-action limit) is valid
    let subset = all_conversation_actions.into_iter().take(5).collect();
    let result = UpdateMobileActions::new(ToolbarType::Conversation, subset);
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
    let result = UpdateMobileActions::new(ToolbarType::List, actions);
    assert!(result.is_err(), "More than 5 actions should be rejected");
}

#[test]
fn test_empty_actions_allowed() {
    let result = UpdateMobileActions::new(ToolbarType::List, vec![]);
    assert!(result.is_ok(), "Empty actions should be allowed");
}

#[test]
fn test_duplicate_actions_allowed() {
    let actions = vec![MobileAction::ToggleRead, MobileAction::ToggleRead];
    let result = UpdateMobileActions::new(ToolbarType::List, actions);
    assert!(
        result.is_ok(),
        "Duplicate actions should be verified on the client side"
    );
}

#[test]
fn test_default_mobile_actions_integration() {
    let default_list_actions = MobileAction::default_chosen_actions();
    let result = UpdateMobileActions::new(ToolbarType::List, default_list_actions.clone());
    assert!(
        result.is_ok(),
        "Should be able to create action for any toolbar type"
    );
    let result = UpdateMobileActions::new(ToolbarType::Message, default_list_actions.clone());
    assert!(
        result.is_ok(),
        "Should be able to create action for any toolbar type"
    );
    let result = UpdateMobileActions::new(ToolbarType::Conversation, default_list_actions);
    assert!(
        result.is_ok(),
        "Should be able to create action for any toolbar type"
    );
}
