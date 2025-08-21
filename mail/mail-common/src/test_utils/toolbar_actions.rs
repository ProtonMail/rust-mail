//! Shared test infrastructure for mobile action testing
//!
//! This module provides common test structures and utilities that can be used
//! across both list action and message action tests to maximize code reuse.

use crate::actions::{ListAction, MessageAction};
use crate::datatypes::{MobileAction, MovableSystemFolder, SystemLabelId, theme::MailTheme};
use crate::decrypted_message::ThemeOpts;
use crate::models::Message;
use proton_core_api::services::proton::LabelId;

/// Unified test case structure that can be used for both list actions and message actions
#[derive(Debug)]
pub struct TestCase<T> {
    /// Current label context for the test
    pub current_local: LabelId,

    /// The item(s) being tested (Message for message actions, Vec<T> for list actions)
    pub test_item: T,

    /// Theme options for testing theme-aware actions
    pub theme: ThemeOpts,

    /// Whether the toolbar is using custom actions
    pub is_custom: bool,

    /// List of toolbar actions
    pub toolbar_actions: Vec<MobileAction>,

    /// Expected visible actions in the result
    pub expected_visible: Vec<TestActions>,

    /// Expected hidden actions in the result
    pub expected_hidden: Vec<TestActions>,
}

impl<T> Default for TestCase<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            current_local: LabelId::inbox(),
            test_item: T::default(),
            theme: ThemeOpts {
                current_theme: MailTheme::LightMode,
                supports_dark_mode_via_media_query: false,
                theme_override: None,
            },
            is_custom: false,
            toolbar_actions: vec![],
            expected_visible: vec![],
            expected_hidden: vec![],
        }
    }
}

/// Unified test actions enum that covers all possible actions for both lists and messages
#[derive(Clone, PartialEq, derive_more::derive::Debug)]
pub enum TestActions {
    // Read state actions (common)
    MarkRead,
    MarkUnread,

    // Star state actions (common)
    Star,
    Unstar,

    // Organization actions (common)
    LabelAs,
    MoveTo,
    MoveToSystemFolder(MovableSystemFolder),
    NotSpam(MovableSystemFolder),
    PermanentDelete,

    // List-specific actions
    Snooze,

    // Communication actions (message only)
    Reply,
    ReplyAll,
    Forward,

    // Export/View actions (message only)
    Print,
    ViewHeaders,
    ViewHTML,
    ViewInLightMode,
    ViewInDarkMode,

    // Utility actions (common)
    ReportPhishing,
    More,
}

impl PartialEq<ListAction> for TestActions {
    fn eq(&self, other: &ListAction) -> bool {
        match (self, other) {
            (TestActions::MarkRead, ListAction::MarkRead) => true,
            (TestActions::MarkUnread, ListAction::MarkUnread) => true,
            (TestActions::Star, ListAction::Star) => true,
            (TestActions::Unstar, ListAction::Unstar) => true,
            (TestActions::LabelAs, ListAction::LabelAs) => true,
            (TestActions::MoveTo, ListAction::MoveTo) => true,
            (TestActions::MoveToSystemFolder(expected), ListAction::MoveToSystemFolder(actual)) => {
                expected == &actual.name
            }
            (TestActions::PermanentDelete, ListAction::PermanentDelete) => true,
            (TestActions::NotSpam(expected), ListAction::NotSpam(actual)) => {
                expected == &actual.name
            }
            (TestActions::Snooze, ListAction::Snooze) => true,
            (TestActions::More, ListAction::More) => true,
            _ => false,
        }
    }
}

impl PartialEq<TestActions> for ListAction {
    fn eq(&self, other: &TestActions) -> bool {
        other == self
    }
}

impl PartialEq<MessageAction> for TestActions {
    fn eq(&self, other: &MessageAction) -> bool {
        match (self, other) {
            (TestActions::MarkRead, MessageAction::MarkRead) => true,
            (TestActions::MarkUnread, MessageAction::MarkUnread) => true,
            (TestActions::Star, MessageAction::Star) => true,
            (TestActions::Unstar, MessageAction::Unstar) => true,
            (TestActions::LabelAs, MessageAction::LabelAs) => true,
            (TestActions::MoveTo, MessageAction::MoveTo) => true,
            (
                TestActions::MoveToSystemFolder(expected),
                MessageAction::MoveToSystemFolder(actual),
            ) => expected == &actual.name,
            (TestActions::NotSpam(expected), MessageAction::NotSpam(actual)) => {
                expected == &actual.name
            }
            (TestActions::PermanentDelete, MessageAction::PermanentDelete) => true,
            (TestActions::Reply, MessageAction::Reply) => true,
            (TestActions::ReplyAll, MessageAction::ReplyAll) => true,
            (TestActions::Forward, MessageAction::Forward) => true,
            (TestActions::Print, MessageAction::Print) => true,
            (TestActions::ViewHeaders, MessageAction::ViewHeaders) => true,
            (TestActions::ViewHTML, MessageAction::ViewHTML) => true,
            (TestActions::ViewInLightMode, MessageAction::ViewInLightMode) => true,
            (TestActions::ViewInDarkMode, MessageAction::ViewInDarkMode) => true,
            (TestActions::ReportPhishing, MessageAction::ReportPhishing) => true,
            (TestActions::More, MessageAction::More) => true,
            _ => false,
        }
    }
}

impl PartialEq<TestActions> for MessageAction {
    fn eq(&self, other: &TestActions) -> bool {
        other == self
    }
}

/// Helper function to create dark mode theme options
pub fn create_dark_mode_theme() -> ThemeOpts {
    ThemeOpts {
        current_theme: MailTheme::DarkMode,
        supports_dark_mode_via_media_query: false,
        theme_override: None,
    }
}

/// Helper function to create a default message test case
pub fn create_default_message_test_case() -> TestCase<Message> {
    TestCase {
        current_local: LabelId::inbox(),
        test_item: Message::test_default(),
        theme: create_dark_mode_theme(),
        is_custom: false,
        toolbar_actions: vec![],
        expected_visible: vec![],
        expected_hidden: vec![],
    }
}

/// Helper function to create a default list test case
pub fn create_default_list_test_case<T>() -> TestCase<Vec<T>> {
    TestCase {
        current_local: LabelId::inbox(),
        test_item: vec![],
        theme: create_dark_mode_theme(),
        is_custom: false,
        toolbar_actions: vec![],
        expected_visible: vec![],
        expected_hidden: vec![],
    }
}
