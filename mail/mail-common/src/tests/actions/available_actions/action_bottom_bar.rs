use crate::actions::BottomBarActions;
use crate::datatypes::{MobileSetting, MobileSettings, MovableSystemFolder, SystemLabelId};
use crate::models::{Conversation, Label, MailSettings, Message};
use proton_core_common::datatypes::{Id, LabelId};
use proton_mail_test_utils::common::create_address;
use proton_mail_test_utils::db::new_test_connection;
use stash::orm::Model;
use std::borrow::ToOwned;
use std::sync::LazyLock;

struct TestCase<T> {
    current_local: LabelId,
    items: Vec<T>,
    is_custom: bool,
    toolbar_actions: Vec<String>,
    expected_visible: Vec<BottomBarActions>,
    expected_hidden: Vec<BottomBarActions>,
}

impl<T> Default for TestCase<T> {
    fn default() -> Self {
        Self {
            current_local: LabelId::inbox(),
            items: vec![],
            is_custom: false,
            toolbar_actions: vec![],
            expected_visible: vec![],
            expected_hidden: vec![],
        }
    }
}

mod message {
    use super::*;
    use crate::datatypes::MovableSystemFolder;
    use test_case::test_case;

    static DEFAULT_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static ALL_UNREAD_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        items: vec![
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
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::Star,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static ALL_READ_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        items: vec![
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
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::Star,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static MIX_READ_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        items: vec![
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
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::Star,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static ALL_STARRED_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        items: vec![
            Message {
                label_ids: vec![LabelId::starred()],
                ..Default::default()
            },
            Message {
                label_ids: vec![LabelId::starred()],
                ..Default::default()
            },
        ],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![BottomBarActions::Unstar, BottomBarActions::More],
        expected_hidden: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static NONE_STARRED_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        items: vec![Message::default(), Message::default()],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![BottomBarActions::Star, BottomBarActions::More],
        expected_hidden: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static CUSTOM_MIX_STARRED_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        items: vec![
            Message {
                label_ids: vec![LabelId::starred()],
                ..Default::default()
            },
            Message::default(),
        ],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![BottomBarActions::Star, BottomBarActions::More],
        expected_hidden: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::Unstar,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static MIX_STARRED_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        items: vec![
            Message {
                label_ids: vec![LabelId::starred()],
                ..Default::default()
            },
            Message::default(),
        ],
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::Star,
            BottomBarActions::Unstar,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static EMPTY_CUSTOM_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        is_custom: true,
        expected_visible: vec![BottomBarActions::More],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static CUSTOM_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        toolbar_actions: vec![
            "archive".to_owned(),
            "label".to_owned(),
            "move".to_owned(),
            "spam".to_owned(),
        ],
        is_custom: true,
        expected_visible: vec![
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::LabelAs,
            BottomBarActions::MoveTo,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::More,
        ],
        expected_hidden: vec![BottomBarActions::MoveToSystemFolder(
            MovableSystemFolder::Trash,
        )],
        ..Default::default()
    });
    static TOO_MANY_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        toolbar_actions: vec![
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
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::LabelAs,
            BottomBarActions::MoveTo,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![],
        ..Default::default()
    });
    static ARCHIVE_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        current_local: LabelId::archive(),
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static TRASH_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        current_local: LabelId::trash(),
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::PermanentDelete,
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
        ],
        ..Default::default()
    });
    static SPAM_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        current_local: LabelId::spam(),
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::PermanentDelete,
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::NotSpam,
        ],
        ..Default::default()
    });

    #[test_case(&DEFAULT_CASE; "default")]
    #[test_case(&ALL_UNREAD_CASE; "unread")]
    #[test_case(&ALL_READ_CASE; "all_read")]
    #[test_case(&MIX_READ_CASE; "mixed_read")]
    #[test_case(&ALL_STARRED_CASE; "all_starred")]
    #[test_case(&CUSTOM_MIX_STARRED_CASE; "mix_custom_starred")]
    #[test_case(&MIX_STARRED_CASE; "mix_starred")]
    #[test_case(&NONE_STARRED_CASE; "none_starred")]
    #[test_case(&EMPTY_CUSTOM_CASE; "empty_custom")]
    #[test_case(&CUSTOM_CASE; "custom")]
    #[test_case(&ARCHIVE_CASE; "archive")]
    #[test_case(&TRASH_CASE; "trash")]
    #[test_case(&SPAM_CASE; "spam")]
    #[test_case(&TOO_MANY_CASE; "too_many")]
    #[tokio::test]
    async fn bottom_bar_actions(test_case: &TestCase<Message>) {
        // Setup
        let stash = new_test_connection().await;

        let mut settings = MailSettings::get_or_default(&stash).await;
        settings.mobile_settings = Some(MobileSettings {
            message_toolbar: MobileSetting {
                actions: test_case.toolbar_actions.clone(),
                is_custom: test_case.is_custom,
            },
            ..Default::default()
        });
        settings.save_using(&stash).await.unwrap();

        let address = create_address(&stash.connection()).await;

        let mut conversation = Conversation::default();
        conversation.save_using(&stash).await.unwrap();

        let mut messages = test_case.items.clone();
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
}

mod conversation {
    use crate::models::ConversationLabel;
    use test_case::test_case;

    use super::*;

    static STARRED_LABEL: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
        remote_label_id: Some(LabelId::starred()),
        ..Default::default()
    });

    static DEFAULT_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static ALL_UNREAD_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                num_unread: 1,
                num_messages: 1,
                ..Default::default()
            },
            Conversation {
                num_unread: 1,
                num_messages: 1,
                ..Default::default()
            },
        ],
        expected_visible: vec![
            BottomBarActions::MarkRead,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::Star,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static ALL_READ_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                num_unread: 0,
                num_messages: 1,
                ..Default::default()
            },
            Conversation {
                num_unread: 0,
                num_messages: 1,
                ..Default::default()
            },
        ],
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::Star,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static MIX_READ_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                num_unread: 0,
                num_messages: 1,
                ..Default::default()
            },
            Conversation {
                num_unread: 1,
                num_messages: 1,
                ..Default::default()
            },
        ],
        expected_visible: vec![
            BottomBarActions::MarkRead,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::Star,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static ALL_STARRED_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                labels: vec![STARRED_LABEL.clone()],
                ..Default::default()
            },
            Conversation {
                labels: vec![STARRED_LABEL.clone()],
                ..Default::default()
            },
        ],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![BottomBarActions::Unstar, BottomBarActions::More],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static NONE_STARRED_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![Conversation::default(), Conversation::default()],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![BottomBarActions::Star, BottomBarActions::More],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static CUSTOM_MIX_STARRED_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                labels: vec![STARRED_LABEL.clone()],
                ..Default::default()
            },
            Conversation::default(),
        ],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![BottomBarActions::Star, BottomBarActions::More],
        expected_hidden: vec![
            BottomBarActions::Unstar,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static MIX_STARRED_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                labels: vec![STARRED_LABEL.clone()],
                ..Default::default()
            },
            Conversation::default(),
        ],
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::Star,
            BottomBarActions::Unstar,
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static EMPTY_CUSTOM_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        is_custom: true,
        expected_visible: vec![BottomBarActions::More],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static CUSTOM_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        toolbar_actions: vec![
            "archive".to_owned(),
            "label".to_owned(),
            "move".to_owned(),
            "spam".to_owned(),
        ],
        is_custom: true,
        expected_visible: vec![
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::LabelAs,
            BottomBarActions::MoveTo,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::More,
        ],
        expected_hidden: vec![BottomBarActions::MoveToSystemFolder(
            MovableSystemFolder::Trash,
        )],
        ..Default::default()
    });
    static TOO_MANY_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        toolbar_actions: vec![
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
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::LabelAs,
            BottomBarActions::MoveTo,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![],
        ..Default::default()
    });
    static ARCHIVE_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        current_local: LabelId::archive(),
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static TRASH_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        current_local: LabelId::trash(),
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::PermanentDelete,
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
        ],
        ..Default::default()
    });
    static SPAM_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        current_local: LabelId::spam(),
        expected_visible: vec![
            BottomBarActions::MarkUnread,
            BottomBarActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            BottomBarActions::PermanentDelete,
            BottomBarActions::More,
        ],
        expected_hidden: vec![
            BottomBarActions::MoveTo,
            BottomBarActions::LabelAs,
            BottomBarActions::NotSpam,
        ],
        ..Default::default()
    });

    #[test_case(&DEFAULT_CASE; "default")]
    #[test_case(&ALL_UNREAD_CASE; "unread")]
    #[test_case(&ALL_READ_CASE; "all_read")]
    #[test_case(&MIX_READ_CASE; "mixed_read")]
    #[test_case(&ALL_STARRED_CASE; "all_starred")]
    #[test_case(&CUSTOM_MIX_STARRED_CASE; "mix_custom_starred")]
    #[test_case(&MIX_STARRED_CASE; "mix_starred")]
    #[test_case(&NONE_STARRED_CASE; "none_starred")]
    #[test_case(&EMPTY_CUSTOM_CASE; "empty_custom")]
    #[test_case(&CUSTOM_CASE; "custom")]
    #[test_case(&ARCHIVE_CASE; "archive")]
    #[test_case(&TRASH_CASE; "trash")]
    #[test_case(&SPAM_CASE; "spam")]
    #[test_case(&TOO_MANY_CASE; "too_many")]
    #[tokio::test]
    async fn bottom_bar_actions(test_case: &TestCase<Conversation>) {
        // Setup
        let stash = new_test_connection().await;

        let mut settings = MailSettings::get_or_default(&stash).await;
        settings.mobile_settings = Some(MobileSettings {
            message_toolbar: MobileSetting {
                actions: test_case.toolbar_actions.clone(),
                is_custom: test_case.is_custom,
            },
            ..Default::default()
        });
        settings.save_using(&stash).await.unwrap();

        let mut conversations = test_case.items.clone();
        for conversation in &mut conversations {
            conversation.save_using(&stash).await.unwrap();
        }
        let current_local = test_case
            .current_local
            .counterpart::<Label, _>(&stash)
            .await
            .unwrap()
            .unwrap();

        // Action
        let result = Conversation::all_available_bottom_bar_actions_for_conversations(
            current_local,
            conversations.iter().map(|m| m.local_id.unwrap()).collect(),
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
}
