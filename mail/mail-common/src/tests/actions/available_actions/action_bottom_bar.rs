use crate::actions::BottomBarActions;
use crate::datatypes::{MobileSetting, MobileSettings, MovableSystemFolder, SystemLabelId};
use crate::models::{Conversation, MailSettings, Message};
use proton_core_api::services::proton::LabelId;
use proton_core_common::models::{Label, ModelIdExtension};
use proton_mail_common::test_utils::db::new_test_connection;
use proton_mail_common::test_utils::utils::create_address;
use std::borrow::ToOwned;
use std::sync::LazyLock;

struct TestCase<T> {
    current_local: LabelId,
    items: Vec<T>,
    is_custom: bool,
    toolbar_actions: Vec<String>,
    expected_visible: Vec<TestActions>,
    expected_hidden: Vec<TestActions>,
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

#[derive(Debug)]
enum TestActions {
    LabelAs,
    MarkRead,
    MarkUnread,
    More,
    MoveTo,
    MoveToSystemFolder(MovableSystemFolder),
    NotSpam(MovableSystemFolder),
    PermanentDelete,
    Star,
    Unstar,
}

impl PartialEq<BottomBarActions> for TestActions {
    fn eq(&self, other: &BottomBarActions) -> bool {
        match self {
            Self::LabelAs => matches!(other, BottomBarActions::LabelAs),
            Self::MarkRead => matches!(other, BottomBarActions::MarkRead),
            Self::MarkUnread => matches!(other, BottomBarActions::MarkUnread),
            Self::More => matches!(other, BottomBarActions::More),
            Self::MoveTo => matches!(other, BottomBarActions::MoveTo),
            Self::MoveToSystemFolder(label) => {
                if let BottomBarActions::MoveToSystemFolder(other) = other {
                    *label == other.name
                } else {
                    false
                }
            }
            Self::NotSpam(label) => {
                if let BottomBarActions::NotSpam(other) = other {
                    *label == other.name
                } else {
                    false
                }
            }
            Self::PermanentDelete => matches!(other, BottomBarActions::PermanentDelete),
            Self::Star => matches!(other, BottomBarActions::Star),
            Self::Unstar => matches!(other, BottomBarActions::Unstar),
        }
    }
}

impl PartialEq<TestActions> for BottomBarActions {
    fn eq(&self, other: &TestActions) -> bool {
        other == self
    }
}

mod message {
    use super::*;
    use crate::datatypes::MovableSystemFolder;
    use stash::stash::StashError;
    use test_case::test_case;

    static DEFAULT_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
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
            TestActions::MarkRead,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
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
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
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
            TestActions::MarkRead,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::Star,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
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
        expected_visible: vec![TestActions::Unstar, TestActions::More],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static NONE_STARRED_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        items: vec![Message::default(), Message::default()],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![TestActions::Star, TestActions::More],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
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
        expected_visible: vec![TestActions::Star, TestActions::More],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::Unstar,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
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
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::Unstar,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static EMPTY_CUSTOM_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        is_custom: true,
        expected_visible: vec![TestActions::More],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
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
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::LabelAs,
            TestActions::MoveTo,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::More,
        ],
        expected_hidden: vec![TestActions::MoveToSystemFolder(MovableSystemFolder::Trash)],
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
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::LabelAs,
            TestActions::MoveTo,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![],
        ..Default::default()
    });
    static ARCHIVE_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        current_local: LabelId::archive(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static TRASH_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        current_local: LabelId::trash(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::PermanentDelete,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
        ],
        ..Default::default()
    });
    static SPAM_CASE: LazyLock<TestCase<Message>> = LazyLock::new(|| TestCase {
        current_local: LabelId::spam(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::PermanentDelete,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::NotSpam(MovableSystemFolder::Inbox),
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
        let messages = tether
            .tx::<_, _, StashError>(async |tx| {
                settings.save(tx).await.unwrap();

                let mut conversation = Conversation::default();
                conversation.save(tx).await.unwrap();

                let mut messages = test_case.items.clone();
                for message in &mut messages {
                    message.local_address_id = address.local_id.unwrap();
                    message.local_conversation_id = conversation.local_id;
                    message.save(tx).await.unwrap();
                }
                Ok(messages)
            })
            .await
            .unwrap();
        let current_local = Label::remote_id_counterpart(test_case.current_local.clone(), &tether)
            .await
            .unwrap()
            .unwrap();

        // Action
        let result = Message::all_available_bottom_bar_actions_for_messages(
            current_local,
            messages.iter().map(|m| m.local_id.unwrap()).collect(),
            &tether,
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
    use super::*;
    use crate::datatypes::ContextualConversation;
    use crate::models::ConversationLabel;
    use stash::stash::StashError;
    use test_case::test_case;

    static INBOX_LABEL_READ: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
        remote_label_id: Some(LabelId::inbox()),
        context_num_unread: 0,
        context_num_messages: 1,
        ..Default::default()
    });
    static INBOX_LABEL_UNREAD: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
        remote_label_id: Some(LabelId::inbox()),
        context_num_unread: 1,
        context_num_messages: 1,
        ..Default::default()
    });
    static TRASH_LABEL_UNREAD: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
        remote_label_id: Some(LabelId::trash()),
        context_num_unread: 1,
        context_num_messages: 1,
        ..Default::default()
    });
    static STARRED_LABEL: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
        remote_label_id: Some(LabelId::starred()),
        ..Default::default()
    });

    static DEFAULT_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static ALL_UNREAD_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                num_unread: 1,
                num_messages: 1,
                labels: vec![INBOX_LABEL_UNREAD.clone()],
                ..Default::default()
            },
            Conversation {
                num_unread: 1,
                num_messages: 1,
                labels: vec![INBOX_LABEL_UNREAD.clone()],
                ..Default::default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkRead,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static ALL_READ_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                num_unread: 0,
                num_messages: 1,
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Default::default()
            },
            Conversation {
                num_unread: 0,
                num_messages: 1,
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Default::default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static MIX_READ_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                num_unread: 0,
                num_messages: 1,
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Default::default()
            },
            Conversation {
                num_unread: 1,
                num_messages: 1,
                labels: vec![INBOX_LABEL_UNREAD.clone()],
                ..Default::default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkRead,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::Star,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
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
        expected_visible: vec![TestActions::Unstar, TestActions::More],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static NONE_STARRED_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Default::default()
            },
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Default::default()
            },
        ],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![TestActions::Star, TestActions::More],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static CUSTOM_MIX_STARRED_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone(), STARRED_LABEL.clone()],
                ..Default::default()
            },
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Default::default()
            },
        ],
        toolbar_actions: vec!["toggle_star".to_owned()],
        is_custom: true,
        expected_visible: vec![TestActions::Star, TestActions::More],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::Unstar,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..Default::default()
    });
    static MIX_STARRED_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone(), STARRED_LABEL.clone()],
                ..Default::default()
            },
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Default::default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::Unstar,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static MIX_MAILBOX_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        items: vec![
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                num_unread: 0,
                ..Default::default()
            },
            Conversation {
                labels: vec![TRASH_LABEL_UNREAD.clone()],
                num_unread: 1,
                ..Default::default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static EMPTY_CUSTOM_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        is_custom: true,
        expected_visible: vec![TestActions::More],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
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
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::LabelAs,
            TestActions::MoveTo,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::More,
        ],
        expected_hidden: vec![TestActions::MoveToSystemFolder(MovableSystemFolder::Trash)],
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
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::LabelAs,
            TestActions::MoveTo,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![],
        ..Default::default()
    });
    static ARCHIVE_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        current_local: LabelId::archive(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..Default::default()
    });
    static TRASH_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        current_local: LabelId::trash(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::PermanentDelete,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
        ],
        ..Default::default()
    });
    static SPAM_CASE: LazyLock<TestCase<Conversation>> = LazyLock::new(|| TestCase {
        current_local: LabelId::spam(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::PermanentDelete,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::NotSpam(MovableSystemFolder::Inbox),
        ],
        ..Default::default()
    });

    #[test_case(&DEFAULT_CASE; "default")]
    #[test_case(&ALL_UNREAD_CASE; "unread")]
    #[test_case(&ALL_READ_CASE; "all_read")]
    #[test_case(&MIX_READ_CASE; "mixed_read")]
    #[test_case(&MIX_MAILBOX_CASE; "mixed_mailbox")]
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
        let mut tether = new_test_connection().await.connection();

        let mut settings = MailSettings::get_or_default(&tether).await;
        settings.mobile_settings = Some(MobileSettings {
            message_toolbar: MobileSetting {
                actions: test_case.toolbar_actions.clone(),
                is_custom: test_case.is_custom,
            },
            ..Default::default()
        });
        let conversations = tether
            .tx::<_, _, StashError>(async |tx| {
                settings.save(tx).await.unwrap();

                let mut conversations = test_case.items.clone();
                for conversation in &mut conversations {
                    conversation.save(tx).await.unwrap();
                }
                Ok(conversations)
            })
            .await
            .unwrap();
        let current_local = Label::remote_id_counterpart(test_case.current_local.clone(), &tether)
            .await
            .unwrap()
            .unwrap();

        // Action
        let result = ContextualConversation::all_available_bottom_bar_actions_for_conversations(
            current_local,
            conversations.iter().map(|m| m.local_id.unwrap()).collect(),
            &tether,
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
