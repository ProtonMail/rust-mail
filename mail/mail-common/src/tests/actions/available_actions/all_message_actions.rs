use crate::datatypes::{
    MessageRecipient, MobileAction, MobileSetting, MobileSettings, MovableSystemFolder,
    SystemLabelId, theme::MailTheme,
};
use crate::decrypted_message::ThemeOpts;
use crate::models::{Conversation, MailSettings, Message};
use proton_core_api::services::proton::LabelId;
use proton_core_common::models::{Label, ModelIdExtension};
use proton_mail_common::test_utils::db::new_test_connection;
use proton_mail_common::test_utils::utils::create_address;
use stash::orm::Model;
use std::sync::LazyLock;
use test_case::test_case;

// Import shared test infrastructure
use crate::test_utils::toolbar_actions::{TestActions, TestCase, create_default_message_test_case};

// Test cases following the same pattern as list actions
static DEFAULT_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    expected_visible: vec![
        TestActions::MarkUnread,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::MoveTo,
        TestActions::LabelAs,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static UNREAD_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    test_item: Message {
        unread: true,
        ..Message::test_default()
    },
    expected_visible: vec![
        TestActions::MarkRead,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::MoveTo,
        TestActions::LabelAs,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static ALL_READ_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    test_item: Message {
        unread: false,
        ..Message::test_default()
    },
    expected_visible: vec![
        TestActions::MarkUnread,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::MoveTo,
        TestActions::LabelAs,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static ALL_STARRED_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    test_item: Message {
        label_ids: vec![LabelId::starred()],
        ..Message::test_default()
    },
    toolbar_actions: vec![MobileAction::ToggleStar],
    is_custom: true,
    expected_visible: vec![TestActions::Unstar, TestActions::More],
    expected_hidden: vec![
        TestActions::MarkUnread,
        TestActions::MoveTo,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::LabelAs,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static NONE_STARRED_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    test_item: Message::test_default(),
    toolbar_actions: vec![MobileAction::ToggleStar],
    is_custom: true,
    expected_visible: vec![TestActions::Star, TestActions::More],
    expected_hidden: vec![
        TestActions::MarkUnread,
        TestActions::MoveTo,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::LabelAs,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static EMPTY_CUSTOM_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    test_item: Message::test_default(),

    is_custom: true,
    expected_visible: vec![TestActions::More],
    expected_hidden: vec![
        TestActions::MarkUnread,
        TestActions::Star,
        TestActions::MoveTo,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::LabelAs,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static CUSTOM_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    test_item: Message::test_default(),
    toolbar_actions: vec![
        MobileAction::Reply,
        MobileAction::Label,
        MobileAction::Move,
        MobileAction::Forward,
    ],
    is_custom: true,
    expected_visible: vec![
        TestActions::Reply,
        TestActions::LabelAs,
        TestActions::MoveTo,
        TestActions::Forward,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::MarkUnread,
        TestActions::Star,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static TOO_MANY_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    toolbar_actions: vec![
        MobileAction::Reply,
        MobileAction::Label,
        MobileAction::Move,
        MobileAction::Forward,
        MobileAction::ToggleRead,
        MobileAction::ToggleStar,
        MobileAction::SavePDF,
    ],
    is_custom: true,
    expected_visible: vec![
        TestActions::Reply,
        TestActions::LabelAs,
        TestActions::MoveTo,
        TestActions::Forward,
        TestActions::MarkUnread,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static ARCHIVE_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    current_local: LabelId::archive(),
    expected_visible: vec![
        TestActions::MarkUnread,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::MoveTo,
        TestActions::LabelAs,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static TRASH_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    current_local: LabelId::trash(),
    expected_visible: vec![
        TestActions::MarkUnread,
        TestActions::PermanentDelete,
        TestActions::MoveTo,
        TestActions::LabelAs,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static SPAM_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    current_local: LabelId::spam(),
    expected_visible: vec![
        TestActions::MarkUnread,
        TestActions::PermanentDelete,
        TestActions::MoveTo,
        TestActions::LabelAs,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::NotSpam(MovableSystemFolder::Inbox),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static REPLY_ALL_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    test_item: Message {
        to_list: vec![
            MessageRecipient::from("user1@example.com"),
            MessageRecipient::from("user2@example.com"),
        ]
        .into(),
        cc_list: vec![MessageRecipient::from("cc@example.com")].into(),
        ..Message::test_default()
    },
    expected_visible: vec![
        TestActions::MarkUnread,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::MoveTo,
        TestActions::LabelAs,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::Reply,
        TestActions::ReplyAll,
        TestActions::Forward,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInLightMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

static DARK_MODE_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
    theme: ThemeOpts {
        current_theme: MailTheme::LightMode,
        supports_dark_mode_via_media_query: false,
        theme_override: None,
    },
    expected_visible: vec![
        TestActions::MarkUnread,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        TestActions::MoveTo,
        TestActions::LabelAs,
        TestActions::More,
    ],
    expected_hidden: vec![
        TestActions::Star,
        TestActions::Reply,
        TestActions::Forward,
        TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        TestActions::Print,
        TestActions::ViewHeaders,
        TestActions::ViewHTML,
        TestActions::ViewInDarkMode,
        TestActions::ReportPhishing,
    ],
    ..create_default_message_test_case()
});

#[test_case(&DEFAULT_CASE; "default")]
#[test_case(&UNREAD_CASE; "unread")]
#[test_case(&ALL_READ_CASE; "all_read")]
#[test_case(&ALL_STARRED_CASE; "all_starred")]
#[test_case(&NONE_STARRED_CASE; "none_starred")]
#[test_case(&EMPTY_CUSTOM_CASE; "empty_custom")]
#[test_case(&CUSTOM_CASE; "custom")]
#[test_case(&ARCHIVE_CASE; "archive")]
#[test_case(&TRASH_CASE; "trash")]
#[test_case(&SPAM_CASE; "spam")]
#[test_case(&TOO_MANY_CASE; "too_many")]
#[test_case(&REPLY_ALL_CASE; "reply_all")]
#[test_case(&DARK_MODE_CASE; "dark_mode")]
#[tokio::test]
async fn message_actions(test_case: &TestCase<Message>) {
    use stash::stash::StashError;

    // Setup
    let mut tether = new_test_connection().await.connection();
    let address = create_address(&mut tether).await;
    let mut settings = MailSettings::get_or_default(&tether).await;
    settings.mobile_settings = Some(MobileSettings {
        message_toolbar: MobileSetting {
            actions: test_case.toolbar_actions.clone(),
            is_custom: test_case.is_custom,
        },
        ..Default::default()
    });

    let message = tether
        .tx::<_, _, StashError>(async |tx| {
            settings.save(tx).await.unwrap();

            let mut conversation = Conversation::test_default();
            conversation.save(tx).await.unwrap();

            let mut message = test_case.test_item.clone();
            message.local_address_id = address.id();
            message.local_conversation_id = conversation.local_id;
            message.save(tx).await.unwrap();
            Ok(message)
        })
        .await
        .unwrap();

    let current_local = Label::remote_id_counterpart(test_case.current_local.clone(), &tether)
        .await
        .unwrap()
        .unwrap();

    // Action
    let result = Message::all_available_message_actions_for_message(
        current_local,
        message.id(),
        test_case.theme,
        &tether,
    )
    .await
    .unwrap();

    // Validation
    assert_eq!(result.visible_message_actions, test_case.expected_visible);
    assert_eq!(result.hidden_message_actions, test_case.expected_hidden);
}

// ============================================================================
// SHARED INFRASTRUCTURE DEMONSTRATION
// ============================================================================
