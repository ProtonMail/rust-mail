use crate::actions::BottomBarActions;
use crate::datatypes::{MobileSetting, MobileSettings, SystemLabel, SystemLabelId};
use crate::models::{Conversation, Label, MailSettings, Message};
use proton_core_common::datatypes::{Id, LabelId};
use proton_mail_test_utils::common::create_address;
use proton_mail_test_utils::db::new_test_connection;
use stash::orm::Model;
use std::borrow::ToOwned;
use std::sync::LazyLock;
use test_case::test_case;

struct TestCase {
    current_local: LabelId,
    messages: Vec<Message>,
    is_custom: bool,
    message_toolbar_actions: Vec<String>,
    expected_visible: Vec<BottomBarActions>,
    expected_hidden: Vec<BottomBarActions>,
}

impl Default for TestCase {
    fn default() -> Self {
        Self {
            current_local: LabelId::inbox(),
            messages: vec![],
            is_custom: false,
            message_toolbar_actions: vec![],
            expected_visible: vec![],
            expected_hidden: vec![],
        }
    }
}

static DEFAULT_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    expected_visible: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static ALL_UNREAD_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    messages: vec![
        Message {
            unread: true,
            ..Default::default()
        },
        Message {
            unread: true,
            ..Default::default()
        },
    ],
    expected_visible: vec![
        BottomBarActions::MarkRead,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::Star,
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static ALL_READ_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    messages: vec![
        Message {
            unread: false,
            ..Default::default()
        },
        Message {
            unread: false,
            ..Default::default()
        },
    ],
    expected_visible: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::Star,
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static MIX_READ_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    messages: vec![
        Message {
            unread: false,
            ..Default::default()
        },
        Message {
            unread: true,
            ..Default::default()
        },
    ],
    expected_visible: vec![
        BottomBarActions::MarkRead,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::Star,
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static ALL_STARRED_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    messages: vec![
        Message {
            label_ids: vec![LabelId::starred()],
            ..Default::default()
        },
        Message {
            label_ids: vec![LabelId::starred()],
            ..Default::default()
        },
    ],
    message_toolbar_actions: vec!["toggle_star".to_owned()],
    is_custom: true,
    expected_visible: vec![BottomBarActions::Unstar, BottomBarActions::More],
    expected_hidden: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static NONE_STARRED_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    messages: vec![Message::default(), Message::default()],
    message_toolbar_actions: vec!["toggle_star".to_owned()],
    is_custom: true,
    expected_visible: vec![BottomBarActions::Star, BottomBarActions::More],
    expected_hidden: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static MIX_STARRED_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    messages: vec![
        Message {
            label_ids: vec![LabelId::starred()],
            ..Default::default()
        },
        Message::default(),
    ],
    message_toolbar_actions: vec!["toggle_star".to_owned()],
    is_custom: true,
    expected_visible: vec![BottomBarActions::Star, BottomBarActions::More],
    expected_hidden: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::Unstar,
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static MIX_STARRED_CASE2: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    messages: vec![
        Message {
            label_ids: vec![LabelId::starred()],
            ..Default::default()
        },
        Message::default(),
    ],
    expected_visible: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::Star,
        BottomBarActions::Unstar,
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static EMPTY_CUSTOM_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    is_custom: true,
    expected_visible: vec![BottomBarActions::More],
    expected_hidden: vec![
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static CUSTOM_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    message_toolbar_actions: vec![
        "archive".to_owned(),
        "label".to_owned(),
        "move".to_owned(),
        "spam".to_owned(),
    ],
    is_custom: true,
    expected_visible: vec![
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::LabelAs,
        BottomBarActions::MoveTo,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static TOO_MANY_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    message_toolbar_actions: vec![
        "archive".to_owned(),
        "label".to_owned(),
        "move".to_owned(),
        "spam".to_owned(),
        "trash".to_owned(),
        "toggle_read".to_owned(),
        "toggle_star".to_owned(),
    ],
    is_custom: true,
    expected_visible: vec![
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::LabelAs,
        BottomBarActions::MoveTo,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::More,
    ],
    expected_hidden: vec![BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed)],
    ..Default::default()
});
static ARCHIVE_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    current_local: LabelId::archive(),
    expected_visible: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Inbox),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Trash),
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Spam),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static TRASH_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    current_local: LabelId::trash(),
    expected_visible: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::PermanentDelete,
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Inbox),
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});
static SPAM_CASE: LazyLock<TestCase> = LazyLock::new(|| TestCase {
    current_local: LabelId::spam(),
    expected_visible: vec![
        BottomBarActions::MarkUnread,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Archive),
        BottomBarActions::PermanentDelete,
        BottomBarActions::More,
    ],
    expected_hidden: vec![
        BottomBarActions::MoveTo,
        BottomBarActions::LabelAs,
        BottomBarActions::NotSpam,
        BottomBarActions::MoveToSystemFolder(SystemLabel::Snoozed),
    ],
    ..Default::default()
});

#[test_case(&DEFAULT_CASE; "default")]
#[test_case(&ALL_UNREAD_CASE; "unread")]
#[test_case(&ALL_READ_CASE; "all_read")]
#[test_case(&MIX_READ_CASE; "mixed_read")]
#[test_case(&ALL_STARRED_CASE; "all_starred")]
#[test_case(&MIX_STARRED_CASE; "mix_custom_starred")]
#[test_case(&MIX_STARRED_CASE2; "mix_starred")]
#[test_case(&NONE_STARRED_CASE; "none_starred")]
#[test_case(&EMPTY_CUSTOM_CASE; "empty_custom")]
#[test_case(&CUSTOM_CASE; "custom")]
#[test_case(&ARCHIVE_CASE; "archive")]
#[test_case(&TRASH_CASE; "trash")]
#[test_case(&SPAM_CASE; "spam")]
#[test_case(&TOO_MANY_CASE; "too_many")]
#[tokio::test]
async fn bottom_bar_actions(test_case: &TestCase) {
    // Setup
    let stash = new_test_connection().await;

    let mut settings = MailSettings::get_or_default(&stash).await;
    settings.mobile_settings = Some(MobileSettings {
        message_toolbar: MobileSetting {
            actions: test_case.message_toolbar_actions.clone(),
            is_custom: test_case.is_custom,
        },
        ..Default::default()
    });
    settings.save_using(&stash).await.unwrap();

    let address = create_address(&stash.connection()).await;

    let mut conversation = Conversation::default();
    conversation.save_using(&stash).await.unwrap();

    let mut messages = test_case.messages.clone();
    for message in &mut messages {
        message.local_address_id = address.local_id.unwrap();
        message.local_conversation_id = conversation.local_id;
        message.save_using(&stash).await.unwrap();
    }
    let current_local = test_case
        .current_local
        .counterpart::<Label, _>(&stash)
        .await
        .unwrap()
        .unwrap();

    // Action
    let result = Message::all_available_bottom_bar_actions_for_messages(
        current_local,
        messages.iter().map(|m| m.local_id.unwrap()).collect(),
        &stash,
    )
    .await
    .unwrap();

    // Validation
    assert_eq!(
        result.visible_bottom_bar_actions,
        test_case.expected_visible
    );
    assert_eq!(result.hidden_bottom_bar_actions, test_case.expected_hidden);
}
