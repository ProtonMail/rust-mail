use crate::datatypes::{
    MobileAction, MobileSetting, MobileSettings, MovableSystemFolder, SystemLabelId,
};
use crate::models::{Conversation, MailSettings, Message};
use proton_core_api::services::proton::LabelId;
use proton_core_common::models::{Label, ModelIdExtension};
use proton_mail_common::test_utils::db::new_test_connection;
use proton_mail_common::test_utils::utils::create_address;
use stash::orm::Model;
use std::sync::LazyLock;

// Import shared test infrastructure
use crate::test_utils::toolbar_actions::{TestActions, TestCase, create_default_list_test_case};

mod message {
    use super::*;
    use crate::datatypes::MovableSystemFolder;
    use stash::stash::StashError;
    use test_case::test_case;

    static DEFAULT_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static ALL_UNREAD_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Message {
                unread: true,
                ..Message::test_default()
            },
            Message {
                unread: true,
                ..Message::test_default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkRead,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static ALL_READ_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Message {
                unread: false,
                ..Message::test_default()
            },
            Message {
                unread: false,
                ..Message::test_default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static MIX_READ_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Message {
                unread: false,
                ..Message::test_default()
            },
            Message {
                unread: true,
                ..Message::test_default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkRead,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::Star,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static ALL_STARRED_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Message {
                label_ids: vec![LabelId::starred()],
                ..Message::test_default()
            },
            Message {
                label_ids: vec![LabelId::starred()],
                ..Message::test_default()
            },
        ],
        toolbar_actions: vec![MobileAction::ToggleStar],
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
        ..create_default_list_test_case()
    });
    static NONE_STARRED_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        test_item: vec![Message::test_default(), Message::test_default()],
        toolbar_actions: vec![MobileAction::ToggleStar],
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
        ..create_default_list_test_case()
    });
    static CUSTOM_MIX_STARRED_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Message {
                label_ids: vec![LabelId::starred()],
                ..Message::test_default()
            },
            Message::test_default(),
        ],
        toolbar_actions: vec![MobileAction::ToggleStar],
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
        ..create_default_list_test_case()
    });
    static MIX_STARRED_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Message {
                label_ids: vec![LabelId::starred()],
                ..Message::test_default()
            },
            Message::test_default(),
        ],
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::Unstar,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static EMPTY_CUSTOM_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        is_custom: true,
        expected_visible: vec![TestActions::More],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..create_default_list_test_case()
    });
    static CUSTOM_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        toolbar_actions: vec![
            MobileAction::Archive,
            MobileAction::Label,
            MobileAction::Move,
            MobileAction::Spam,
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
        ..create_default_list_test_case()
    });
    static TOO_MANY_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        toolbar_actions: vec![
            MobileAction::Archive,
            MobileAction::Label,
            MobileAction::Move,
            MobileAction::Spam,
            MobileAction::Trash,
            MobileAction::ToggleRead,
            MobileAction::ToggleStar,
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
        ..create_default_list_test_case()
    });
    static ARCHIVE_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        current_local: LabelId::archive(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static TRASH_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        current_local: LabelId::trash(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::PermanentDelete,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        ],
        ..create_default_list_test_case()
    });
    static SPAM_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        current_local: LabelId::spam(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::PermanentDelete,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::NotSpam(MovableSystemFolder::Inbox),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        ],
        ..create_default_list_test_case()
    });
    static HIDE_SNOOZE_CASE: LazyLock<TestCase<Vec<Message>>> = LazyLock::new(|| TestCase {
        test_item: vec![Message::test_default(), Message::test_default()],
        toolbar_actions: vec![MobileAction::ToggleStar, MobileAction::Snooze],
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
        ..create_default_list_test_case()
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
    #[test_case(&HIDE_SNOOZE_CASE; "hide_snooze")]
    #[tokio::test]
    async fn bottom_bar_actions(test_case: &TestCase<Vec<Message>>) {
        // Setup
        let stash = new_test_connection().await;
        let mut tether = stash.connection().await.unwrap();
        let address = create_address(&mut tether).await;
        let mut settings = MailSettings::get_or_default(&tether).await;
        settings.mobile_settings = Some(MobileSettings {
            list_toolbar: MobileSetting {
                actions: test_case.toolbar_actions.clone(),
                is_custom: test_case.is_custom,
            },
            ..Default::default()
        });
        let messages = tether
            .tx::<_, _, StashError>(async |tx| {
                settings.save(tx).await.unwrap();

                let mut conversation = Conversation::test_default();
                conversation.save(tx).await.unwrap();

                let mut messages = test_case.test_item.clone();
                for message in &mut messages {
                    message.local_address_id = address.id();
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
        let result = Message::all_available_list_actions_for_messages(
            current_local,
            messages.iter().map(|m| m.id()).collect(),
            &tether,
        )
        .await
        .unwrap();

        // Validation
        assert_eq!(result.visible_list_actions, test_case.expected_visible);
        assert_eq!(result.hidden_list_actions, test_case.expected_hidden);
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
        ..ConversationLabel::test_default()
    });
    static INBOX_LABEL_UNREAD: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
        remote_label_id: Some(LabelId::inbox()),
        context_num_unread: 1,
        context_num_messages: 1,
        ..ConversationLabel::test_default()
    });
    static TRASH_LABEL_UNREAD: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
        remote_label_id: Some(LabelId::trash()),
        context_num_unread: 1,
        context_num_messages: 1,
        ..ConversationLabel::test_default()
    });
    static STARRED_LABEL: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
        remote_label_id: Some(LabelId::starred()),
        ..ConversationLabel::test_default()
    });

    static DEFAULT_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Snooze,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static ALL_UNREAD_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Conversation {
                num_unread: 1,
                num_messages: 1,
                labels: vec![INBOX_LABEL_UNREAD.clone()],
                ..Conversation::test_default()
            },
            Conversation {
                num_unread: 1,
                num_messages: 1,
                labels: vec![INBOX_LABEL_UNREAD.clone()],
                ..Conversation::test_default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkRead,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::Snooze,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static ALL_READ_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Conversation {
                num_unread: 0,
                num_messages: 1,
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Conversation::test_default()
            },
            Conversation {
                num_unread: 0,
                num_messages: 1,
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Conversation::test_default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::Snooze,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static MIX_READ_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Conversation {
                num_unread: 0,
                num_messages: 1,
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Conversation::test_default()
            },
            Conversation {
                num_unread: 1,
                num_messages: 1,
                labels: vec![INBOX_LABEL_UNREAD.clone()],
                ..Conversation::test_default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkRead,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::Star,
            TestActions::Snooze,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static ALL_STARRED_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Conversation {
                labels: vec![STARRED_LABEL.clone()],
                ..Conversation::test_default()
            },
            Conversation {
                labels: vec![STARRED_LABEL.clone()],
                ..Conversation::test_default()
            },
        ],
        toolbar_actions: vec![MobileAction::ToggleStar],
        is_custom: true,
        expected_visible: vec![TestActions::Unstar, TestActions::More],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::Snooze,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..create_default_list_test_case()
    });
    static NONE_STARRED_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Conversation::test_default()
            },
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Conversation::test_default()
            },
        ],
        toolbar_actions: vec![MobileAction::ToggleStar],
        is_custom: true,
        expected_visible: vec![TestActions::Star, TestActions::More],
        expected_hidden: vec![
            TestActions::MarkUnread,
            TestActions::MoveTo,
            TestActions::Snooze,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..create_default_list_test_case()
    });
    static CUSTOM_MIX_STARRED_CASE: LazyLock<TestCase<Vec<Conversation>>> =
        LazyLock::new(|| TestCase {
            test_item: vec![
                Conversation {
                    labels: vec![INBOX_LABEL_READ.clone(), STARRED_LABEL.clone()],
                    ..Conversation::test_default()
                },
                Conversation {
                    labels: vec![INBOX_LABEL_READ.clone()],
                    ..Conversation::test_default()
                },
            ],
            toolbar_actions: vec![MobileAction::ToggleStar],
            is_custom: true,
            expected_visible: vec![TestActions::Star, TestActions::More],
            expected_hidden: vec![
                TestActions::MarkUnread,
                TestActions::Unstar,
                TestActions::MoveTo,
                TestActions::Snooze,
                TestActions::LabelAs,
                TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
                TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
                TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            ],
            ..create_default_list_test_case()
        });
    static MIX_STARRED_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone(), STARRED_LABEL.clone()],
                ..Conversation::test_default()
            },
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                ..Conversation::test_default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::Unstar,
            TestActions::Snooze,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static MIX_MAILBOX_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        test_item: vec![
            Conversation {
                labels: vec![INBOX_LABEL_READ.clone()],
                num_unread: 0,
                ..Conversation::test_default()
            },
            Conversation {
                labels: vec![TRASH_LABEL_UNREAD.clone()],
                num_unread: 1,
                ..Conversation::test_default()
            },
        ],
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Star,
            TestActions::Snooze,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static EMPTY_CUSTOM_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        is_custom: true,
        expected_visible: vec![TestActions::More],
        expected_hidden: vec![
            TestActions::MoveTo,
            TestActions::Snooze,
            TestActions::LabelAs,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..create_default_list_test_case()
    });
    static CUSTOM_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        toolbar_actions: vec![
            MobileAction::Archive,
            MobileAction::Label,
            MobileAction::Move,
            MobileAction::Spam,
        ],
        is_custom: true,
        expected_visible: vec![
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
            TestActions::LabelAs,
            TestActions::MoveTo,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::Snooze,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
        ],
        ..create_default_list_test_case()
    });
    static TOO_MANY_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        toolbar_actions: vec![
            MobileAction::Archive,
            MobileAction::Label,
            MobileAction::Move,
            MobileAction::Spam,
            MobileAction::Trash,
            MobileAction::ToggleRead,
            MobileAction::ToggleStar,
            MobileAction::Snooze,
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
        expected_hidden: vec![TestActions::Snooze],
        ..create_default_list_test_case()
    });
    static ARCHIVE_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        current_local: LabelId::archive(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
        ],
        ..create_default_list_test_case()
    });
    static TRASH_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        current_local: LabelId::trash(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::PermanentDelete,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::MoveToSystemFolder(MovableSystemFolder::Inbox),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        ],
        ..create_default_list_test_case()
    });
    static SPAM_CASE: LazyLock<TestCase<Vec<Conversation>>> = LazyLock::new(|| TestCase {
        current_local: LabelId::spam(),
        expected_visible: vec![
            TestActions::MarkUnread,
            TestActions::PermanentDelete,
            TestActions::MoveTo,
            TestActions::LabelAs,
            TestActions::More,
        ],
        expected_hidden: vec![
            TestActions::NotSpam(MovableSystemFolder::Inbox),
            TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
        ],
        ..create_default_list_test_case()
    });
    static CUSTOM_SNOOZE_AT_THE_BOTTOM_CASE: LazyLock<TestCase<Vec<Conversation>>> =
        LazyLock::new(|| TestCase {
            toolbar_actions: vec![
                MobileAction::Archive,
                MobileAction::Label,
                MobileAction::Move,
                MobileAction::Snooze,
            ],
            is_custom: true,
            expected_visible: vec![
                TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
                TestActions::LabelAs,
                TestActions::MoveTo,
                TestActions::Snooze,
                TestActions::More,
            ],
            expected_hidden: vec![
                TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
                TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            ],
            ..create_default_list_test_case()
        });
    static CUSTOM_SNOOZE_AT_THE_TOP_CASE: LazyLock<TestCase<Vec<Conversation>>> =
        LazyLock::new(|| TestCase {
            toolbar_actions: vec![
                MobileAction::Snooze,
                MobileAction::Archive,
                MobileAction::Label,
                MobileAction::Move,
            ],
            is_custom: true,
            expected_visible: vec![
                TestActions::Snooze,
                TestActions::MoveToSystemFolder(MovableSystemFolder::Archive),
                TestActions::LabelAs,
                TestActions::MoveTo,
                TestActions::More,
            ],
            expected_hidden: vec![
                TestActions::MoveToSystemFolder(MovableSystemFolder::Spam),
                TestActions::MoveToSystemFolder(MovableSystemFolder::Trash),
            ],
            ..create_default_list_test_case()
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
    #[test_case(&CUSTOM_SNOOZE_AT_THE_BOTTOM_CASE; "custom_snooze_at_the_bottom")]
    #[test_case(&CUSTOM_SNOOZE_AT_THE_TOP_CASE; "custom_snooze_at_the_top")]
    #[tokio::test]
    async fn bottom_bar_actions(test_case: &TestCase<Vec<Conversation>>) {
        // Setup
        let stash = new_test_connection().await;
        let mut tether = stash.connection().await.unwrap();

        let mut settings = MailSettings::get_or_default(&tether).await;
        settings.mobile_settings = Some(MobileSettings {
            list_toolbar: MobileSetting {
                actions: test_case.toolbar_actions.clone(),
                is_custom: test_case.is_custom,
            },
            ..Default::default()
        });
        let conversations = tether
            .tx::<_, _, StashError>(async |tx| {
                settings.save(tx).await.unwrap();

                let mut conversations = test_case.test_item.clone();
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
        let result = ContextualConversation::all_available_list_actions_for_conversations(
            current_local,
            conversations.iter().map(|m| m.id()).collect(),
            &tether,
        )
        .await
        .unwrap();

        // Validation
        assert_eq!(result.visible_list_actions, test_case.expected_visible);
        assert_eq!(result.hidden_list_actions, test_case.expected_hidden);
    }
}
