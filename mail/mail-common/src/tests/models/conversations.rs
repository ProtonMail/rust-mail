#![allow(non_snake_case)]

use super::super::*;
use crate as proton_mail_common;
use crate::datatypes::MovableSystemFolder;
use crate::datatypes::{
    ContextualConversation, ConversationCount, LabelType, MessageAddress, MessageFlags,
    SystemLabelId,
};
use lazy_static::lazy_static;
use pretty_assertions::assert_eq;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::response_data::{
    AttachmentMetadata as ApiAttachmentMetadata, ConversationLabel as ApiConversationLabel,
    Disposition as ApiDisposition,
};
use proton_core_common::datatypes::{Id, LabelId};
use proton_mail_test_utils::db::new_test_connection_file;
use proton_mail_test_utils::db_states::{
    new_test_delete_db_state, new_test_label_db_state,
    new_test_label_db_state_label_with_existing_labels, new_test_unread_db_state,
};
use proton_mail_test_utils::label;
use proton_mail_test_utils::search::{
    create_address, create_labels, test_conversation, test_starred_label, MY_ATTACHMENT_ID,
    MY_LABEL_ID1, MY_LABEL_ID2,
};
use proton_mail_test_utils::utils::{
    conv_counts_as_map, message_counts_for_conversation, msg_counts_as_map,
    prepare_and_patch_db_state, prepare_and_patch_db_state_and_skip, prepare_db_state_core,
};
use stash::orm::Model;
use stash::params;

mod first_unread_message {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    lazy_static! {
        static ref STARRED: Label = new_label(LabelType::System, Some(LabelId::starred().clone()));
        static ref LABEL: Label = new_label(LabelType::Label, Some("label".into()));
        static ref FOLDER: Label = new_label(LabelType::Folder, Some("folder".into()));
        static ref INBOX: Label = new_label(LabelType::System, Some(LabelId::inbox().clone()));
        static ref DRAFTS: Label = new_label(LabelType::System, Some(LabelId::drafts().clone())); // There is no conversations in drafts - this is theoretical case
        static ref ALL_LABELS: Vec<&'static Label> =
            vec![&STARRED, &LABEL, &FOLDER, &INBOX, &DRAFTS];
        static ref MOVED_CONV_LABELS: Vec<&'static Label> =
            vec![&STARRED, &LABEL, &FOLDER];
        static ref INBOX_AND_DRAFTS_LABELS: Vec<&'static Label> = vec![&INBOX, &DRAFTS];
    }

    #[test_case(
    &ALL_LABELS, &[], None; "TEST1 - empty messages"
)]
    #[test_case(
    &ALL_LABELS, &[(MessageFlags::RECEIVED, false, &ALL_LABELS),], Some(0.into()); "TEST2 - read - received message"
)]
    #[test_case(
    &ALL_LABELS, &[(MessageFlags::empty(), false, &ALL_LABELS),], None; "TEST3 - read - draft message"
)]
    #[test_case(
    &ALL_LABELS, &[(MessageFlags::OPENED, false, &ALL_LABELS),], None; "TEST4 - read - draft & opened message"
)]
    #[test_case(
    &ALL_LABELS, &[(MessageFlags::OPENED, true, &ALL_LABELS),], None; "TEST5 - unread - draft & opened message"
)]
    #[test_case(
    &ALL_LABELS, &[(MessageFlags::RECEIVED | MessageFlags::OPENED, true, &ALL_LABELS),], Some(0.into()); "TEST6 - unread - received & opened message"
)]
    #[test_case(
    &ALL_LABELS, &[(MessageFlags::RECEIVED, true, &ALL_LABELS),], Some(0.into()); "TEST7 - unread - received message"
)]
    #[test_case(
    &ALL_LABELS, &[(MessageFlags::RECEIVED | MessageFlags::INTERNAL, true, &ALL_LABELS),], Some(0.into()); "TEST8 - unread - received & internal message"
)]
    #[test_case(
    &ALL_LABELS, &[(MessageFlags::SENT | MessageFlags::INTERNAL, true, &ALL_LABELS),], Some(0.into()); "TEST9 - unread - opened & internal message"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::RECEIVED | MessageFlags::INTERNAL | MessageFlags::OPENED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED | MessageFlags::INTERNAL, true, &ALL_LABELS),

    ], Some(2.into()); "TEST10 - all unread - received | internal | opened messages"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::empty(), true, &ALL_LABELS),

    ], Some(0.into()); "TEST11 - all unread - received | draft messages"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::empty(), false, &ALL_LABELS),

    ], Some(0.into()); "TEST12 - some unread - received | draft messages"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::SENT, true, &ALL_LABELS),
        (MessageFlags::SENT, true, &ALL_LABELS),
        (MessageFlags::empty(), false, &ALL_LABELS),

    ], Some(0.into()); "TEST13 - some unread - sent | draft messages"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::SENT | MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::SENT | MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::empty(), false, &ALL_LABELS),

    ], Some(0.into()); "TEST14 - some unread - sent & received | draft messages"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::empty(), true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::empty(), true, &ALL_LABELS),

    ], Some(3.into()); "TEST15 - all unread - received | draft messages"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
    ], Some(2.into()); "TEST16 - first_unread_conversation_message_in_starred_or_custom_label_or_folder"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::empty(), true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
    ], Some(3.into()); "TEST17 - first_unread_conversation_message_in_starred_or_custom_label_or_folder_non_consecutive_with_draft"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::empty(), true, &ALL_LABELS),
    ], Some(2.into()); "TEST18 - first_unread_conversation_message_in_starred_or_custom_label_or_folder_non_consecutive_with_draft"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::empty(), true, &ALL_LABELS),
    ], Some(0.into()); "TEST19 - first_unread_conversation_message_in_starred_or_custom_label_or_folder_non_consecutive_with_draft"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
    ], Some(2.into()); "TEST20 - first_unread_conversation_message_default_last_consecutive_unread"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::empty(), true, &ALL_LABELS),
    ], Some(2.into()); "TEST21 - first_unread_conversation_message_default_last_consecutive_unread_if_last_is_draft_or_auto_send"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::SENT | MessageFlags::AUTO, true, &ALL_LABELS),
    ], Some(2.into()); "TEST22 - first_unread_conversation_message_default_last_consecutive_unread_if_last_is_draft_or_auto_send"
)]
    #[test_case(
    &MOVED_CONV_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::SENT | MessageFlags::AUTO, true, &ALL_LABELS),
        (MessageFlags::empty(), true, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
    ], Some(2.into()); "TEST23A - first_unread_conversation_message_default_last_nonconsecutive_not_draft_or_auto_send"
)]
    #[test_case(
    &INBOX_AND_DRAFTS_LABELS, &[
        (MessageFlags::RECEIVED, true, &INBOX_AND_DRAFTS_LABELS),
        (MessageFlags::RECEIVED, false, &INBOX_AND_DRAFTS_LABELS),
        (MessageFlags::SENT | MessageFlags::AUTO, true, &INBOX_AND_DRAFTS_LABELS),
        (MessageFlags::empty(), true, &INBOX_AND_DRAFTS_LABELS),
        (MessageFlags::RECEIVED, false, &INBOX_AND_DRAFTS_LABELS),
    ], Some(0.into()); "TEST23B - first_unread_conversation_message_default_last_nonconsecutive_not_draft_or_auto_send"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
        (MessageFlags::RECEIVED, true, &ALL_LABELS),
    ], Some(0.into()); "TEST24 - oldest_unread_message_selected_in_unread_chain"
)]
    #[test_case(
    &ALL_LABELS, &[
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
        (MessageFlags::RECEIVED, false, &ALL_LABELS),
    ], Some(2.into()); "TEST25 - all read"
)]
    #[test_case(
    &[&INBOX], &[
        (MessageFlags::RECEIVED, false, &[&INBOX]),
        (MessageFlags::RECEIVED, false, &[&INBOX]),
        (MessageFlags::RECEIVED, false, &[&FOLDER]),
    ], Some(1.into()); "TEST26 - different view labels"
)]
    #[test_case(
    &[&INBOX], &[
        (MessageFlags::RECEIVED, false, &[&INBOX]),
        (MessageFlags::RECEIVED, false, &[&FOLDER]),
        (MessageFlags::RECEIVED, false, &[&INBOX]),
    ], Some(2.into()); "TEST27 - different view labels"
)]
    #[test_case(
    &[&INBOX], &[
        (MessageFlags::RECEIVED, false, &[&FOLDER]),
        (MessageFlags::RECEIVED, false, &[&INBOX]),
        (MessageFlags::RECEIVED, false, &[&INBOX]),
    ], Some(2.into()); "TEST28 - different view labels"
)]
    fn first_unread_message(
        labels: &[&Label],
        messages: &[(MessageFlags, bool, &[&Label])],
        expected_id: Option<LocalId>,
    ) {
        let messages = messages
            .iter()
            .enumerate()
            .map(|(id, (flags, unread, labels))| {
                message_metadata_with_flags((id as u64).into(), *flags, *unread, labels)
            })
            .collect::<Vec<_>>();

        for label in labels {
            assert_eq!(
                Conversation::first_unread_message(label, &messages),
                expected_id,
                "Test failed for label: {:?}, {:?}",
                label.label_type,
                label.remote_id
            );
        }
    }

    fn message_metadata_with_flags(
        id: LocalId,
        flags: MessageFlags,
        unread: bool,
        labels: &[&Label],
    ) -> Message {
        let label_ids = labels
            .iter()
            .map(|label| label.remote_id.clone().unwrap())
            .collect();

        Message {
            local_id: Some(id),
            unread,
            sender: MessageAddress {
                address: String::new(),
                bimi_selector: None,
                display_sender_image: false,
                is_proton: false,
                is_simple_login: false,
                name: String::new(),
            },
            flags,
            label_ids,
            ..Default::default()
        }
    }

    fn new_label(label_type: LabelType, rid: Option<LabelId>) -> Label {
        label!(label_type, remote_id: rid)
    }
}

mod available_actions {
    use std::sync::LazyLock;

    use super::*;
    use crate::actions::ConversationAvailableActions;
    use crate::actions::MovableSystemFolderAction;
    use crate::datatypes::MovableSystemFolder;
    use pretty_assertions::assert_eq;
    use proton_mail_test_utils::db::new_test_connection;
    use proton_mail_test_utils::{conversation, rid};
    use test_case::test_case;

    lazy_static! {
        static ref STARRED: Label =
            label!(label_type: LabelType::System, remote_id: Some(LabelId::starred()));
        static ref FOLDER: Label = label!(label_type: LabelType::Folder, remote_id: Some("folder_label".into()), name: "MyFavouritesFolder".to_owned(), color: LabelColor::black());
        static ref INBOX: Label = label!(label_type: LabelType::System, remote_id: Some(LabelId::inbox()), name: "Inbox".to_owned(), color: LabelColor::black());
        static ref SPAM: Label = label!(label_type: LabelType::System, remote_id: Some(LabelId::spam()), name: "Spam".to_owned(), color: LabelColor::black());
        static ref ARCHIVE: Label = label!(label_type: LabelType::System, remote_id: Some(LabelId::archive()), name: "Archive".to_owned(), color: LabelColor::black());
        static ref TRASH: Label = label!(label_type: LabelType::System, remote_id: Some(LabelId::trash()), name: "Trash".to_owned(), color: LabelColor::black());
        static ref ALL_MAIL: Label =
            label!(label_type: LabelType::System, remote_id: Some(LabelId::all_mail()));
        static ref APPLICABLE_LABEL_1: Label = label!(label_type: LabelType::Label, remote_id: Some("applicable_label_1".into()), name: "Applicable Label 1".to_owned(), color: LabelColor::purple());
        static ref APPLICABLE_LABEL_2: Label = label!(label_type: LabelType::Label, remote_id: Some("applicable_label_2".into()), name: "Applicable Label 2".to_owned(), color: LabelColor::purple());
        static ref APPLICABLE_LABEL_3: Label = label!(label_type: LabelType::Label, remote_id: Some("applicable_label_3".into()), name: "Applicable Label 3".to_owned(), color: LabelColor::purple());
    }

    struct TestCase {
        view: Label,
        conversations: Vec<ConversationWithLabels>,
        expected: Result<ConversationAvailableActions, AppError>,
    }

    #[derive(Clone)]
    struct ConversationWithLabels {
        conversation: Conversation,
        labels: Vec<Label>,
    }

    static TEST0: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: INBOX.clone(),
        conversations: vec![],
        expected: Err(AppError::EmptyListOfConversations),
    });

    static TEST1: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: INBOX.clone(),
        conversations: vec![ConversationWithLabels {
            conversation: conversation!(deleted: false, num_unread: 1, remote_id: rid!("conversation1")),
            labels: vec![STARRED.clone(), FOLDER.clone()],
        }],
        expected: Err(AppError::ConversationDoesNotHaveLabel(
            1.into(),
            "Inbox".to_string(),
        )),
    });

    static TEST2: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: INBOX.clone(),
        conversations: vec![ConversationWithLabels {
            conversation: conversation!(deleted: false, num_unread: 1, remote_id: rid!("conversation_1")),
            labels: vec![STARRED.clone(), INBOX.clone()],
        }],
        expected: Ok(ConversationAvailableActions::builder()
            .move_actions(vec![
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Spam,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Trash,
                    is_selected: Some(false),
                },
            ])
            .conversation_actions(vec![
                ConversationAction::Unstar,
                ConversationAction::MarkRead,
                ConversationAction::Pin,
                ConversationAction::LabelAs,
                ConversationAction::Delete,
            ])
            .build()),
    });

    static TEST3: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: FOLDER.clone(),
        conversations: vec![ConversationWithLabels {
            conversation: conversation!(deleted: true, num_unread: 0, remote_id: Some("test2".into())),
            labels: vec![FOLDER.clone()],
        }],
        expected: Ok(ConversationAvailableActions::builder()
            .move_actions(vec![
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Inbox,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Spam,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Trash,
                    is_selected: Some(false),
                },
            ])
            .conversation_actions(vec![
                ConversationAction::Star,
                ConversationAction::MarkUnread,
                ConversationAction::Pin,
                ConversationAction::LabelAs,
            ])
            .build()),
    });

    static TEST4: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: SPAM.clone(),
        conversations: vec![ConversationWithLabels {
            conversation: conversation!(remote_id: Some("test3".into())),
            labels: vec![SPAM.clone()],
        }],
        expected: Ok(ConversationAvailableActions::builder()
            .move_actions(vec![
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Inbox,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Trash,
                    is_selected: Some(false),
                },
            ])
            .conversation_actions(vec![
                ConversationAction::Star,
                ConversationAction::MarkUnread,
                ConversationAction::Pin,
                ConversationAction::LabelAs,
                ConversationAction::Delete,
            ])
            .build()),
    });

    static TEST5: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: INBOX.clone(),
        conversations: vec![
            ConversationWithLabels {
                conversation: conversation!(deleted: true, num_unread: 0, remote_id: Some("test4_1".into())),
                labels: vec![STARRED.clone(), INBOX.clone()],
            },
            ConversationWithLabels {
                conversation: conversation!(deleted: false, num_unread: 1, remote_id: Some("test4_2".into())),
                labels: vec![INBOX.clone()],
            },
        ],
        expected: Ok(ConversationAvailableActions::builder()
            .move_actions(vec![
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Spam,
                    is_selected: Some(false),
                },
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Trash,
                    is_selected: Some(false),
                },
            ])
            .conversation_actions(vec![
                ConversationAction::Star,
                ConversationAction::MarkRead,
                ConversationAction::Pin,
                ConversationAction::LabelAs,
                ConversationAction::Delete,
            ])
            .build()),
    });

    #[test_case(&TEST0; "TEST0: empty")]
    #[test_case(&TEST1; "TEST1: Unread, starred in custom folder viewed from Inbox")]
    #[test_case(&TEST2; "TEST2: Unread, starred in Inbox viewed from Inbox")]
    #[test_case(&TEST3; "TEST3: Read, not starred, deleted and in custom folder viewed from Folder")]
    #[test_case(&TEST4; "TEST4: Default, viewed from Spam")]
    #[test_case(&TEST5; "TEST5: Two conversations, one from TEST1 and other from TEST2")]
    #[tokio::test]
    async fn test_available_actions(test_case: &TestCase) {
        let stash = new_test_connection().await;
        let tx = stash.connection();
        let mut conversation_ids = vec![];

        for ConversationWithLabels {
            mut conversation,
            labels,
        } in test_case.conversations.clone()
        {
            conversation
                .save_using(&tx)
                .await
                .expect("failed to create conversation");

            conversation_ids.push(conversation.local_id.unwrap());

            for mut label in labels {
                label.save_using(&tx).await.expect("failed to create label");

                let label_id = label.local_id.unwrap();
                let ids = vec![conversation.local_id.unwrap()];

                Conversation::apply_label(label_id, ids, &tx).await.unwrap();
            }
        }

        let view = Label::find_by_id(test_case.view.remote_id.clone().unwrap().into_inner(), &tx)
            .await
            .unwrap()
            .unwrap();

        let result = Conversation::available_actions(view, conversation_ids, &tx).await;

        match result {
            Ok(mut actual) => {
                actual.move_actions.iter_mut().for_each(|action| {
                    action.local_id = 0.into(); // To be able to compare with expected
                });

                assert_eq!(&actual, test_case.expected.as_ref().unwrap());
            }
            Err(err) => {
                assert_eq!(
                    err.to_string(),
                    test_case.expected.as_ref().unwrap_err().to_string()
                );
            }
        }
    }
}

mod available_label_as_actions {
    use super::*;
    use pretty_assertions::assert_eq;
    use proton_mail_test_utils::db::new_test_connection;
    use proton_mail_test_utils::{conversation, label, rid};
    use test_case::test_case;

    struct ConversationWithLabels {
        conversation: Conversation,
        labels: Vec<Label>,
    }

    #[test_case(vec![], vec![], Err(AppError::EmptyListOfConversations); "TEST1: empty")]
    #[test_case(
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![] },
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_2")), labels: vec![] },
        ],
        vec![
            label!(remote_id: rid!("label1"), label_type: LabelType::Label, name: "label1".to_string(), color: LabelColor::purple()),
            label!(remote_id: rid!("label2"), label_type: LabelType::Label, name: "label2".to_string()),
        ],
        Ok(&[
            LabelAsAction {
                label_id: 0.into(),
                name: "label1".into(),
                color: LabelColor::purple(),
                is_selected: Some( false )
            },
            LabelAsAction {
                label_id: 0.into(),
                name: "label2".into(),
                color: Default::default(),
                is_selected: Some( false )
            }
        ]); "TEST2: conversations without labels")]
    #[test_case(
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![
                label!(remote_id: rid!("label1"), label_type: LabelType::Label, name: "label1".to_string(), color: LabelColor::purple()),
                label!(remote_id: rid!("label2"), label_type: LabelType::Label, name: "label2".to_string()),
            ] },
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_2")), labels: vec![
                label!(remote_id: rid!("label1"), label_type: LabelType::Label, name: "label1".to_string(), color: LabelColor::purple()),
                label!(remote_id: rid!("label2"), label_type: LabelType::Label, name: "label2".to_string()),
            ] },
        ],
        vec![],
        Ok(&[
            LabelAsAction {
                label_id: 0.into(),
                name: "label1".into(),
                color: LabelColor::purple(),
                is_selected: Some( true )
            },
            LabelAsAction {
                label_id: 0.into(),
                name: "label2".into(),
                color: Default::default(),
                is_selected: Some( true )
            }
        ]); "TEST3: conversations with all labels")]
    #[test_case(
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![
                label!(remote_id: rid!("label1"), label_type: LabelType::Label, name: "label1".to_string(), color: LabelColor::purple()),
            ] },
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_2")), labels: vec![
                label!(remote_id: rid!("label2"), label_type: LabelType::Label, name: "label2".to_string()),
            ] },
        ],
        vec![],
        Ok(&[
            LabelAsAction {
                label_id: 0.into(),
                name: "label1".into(),
                color: LabelColor::purple(),
                is_selected: None,
            },
            LabelAsAction {
                label_id: 0.into(),
                name: "label2".into(),
                color: Default::default(),
                is_selected: None,
            }
        ]); "TEST4: each conversation with different label")]
    #[tokio::test]
    async fn test_label_as_actions(
        conversations: Vec<ConversationWithLabels>,
        labels: Vec<Label>,
        expected: Result<&[LabelAsAction], AppError>,
    ) {
        let stash = new_test_connection().await;
        let tx = stash.connection();

        for mut label in labels {
            label.save_using(&tx).await.expect("failed to create label");
        }

        let mut conversation_ids = vec![];

        for ConversationWithLabels {
            mut conversation,
            labels: message_labels,
        } in conversations
        {
            conversation
                .save_using(&tx)
                .await
                .expect("failed to create message");

            conversation_ids.push(conversation.local_id.unwrap());

            for mut label in message_labels {
                label.save_using(&tx).await.expect("failed to create label");

                let label_id = label.local_id.unwrap();
                let ids = vec![conversation.local_id.unwrap()];

                Conversation::apply_label(label_id, ids, &tx).await.unwrap();
            }
        }

        let result = Conversation::available_label_as_actions(conversation_ids, &tx).await;

        match result {
            Ok(mut actual) => {
                actual.iter_mut().for_each(|action| {
                    action.label_id = 0.into(); // To be able to compare with expected
                });

                assert_eq!(actual, expected.unwrap());
            }
            Err(err) => {
                assert_eq!(err.to_string(), expected.unwrap_err().to_string());
            }
        }
    }
}

mod available_move_to_actions {
    use super::*;
    use crate::datatypes::SystemLabel;
    use futures::stream::{self, StreamExt};
    use pretty_assertions::assert_eq;
    use proton_mail_test_utils::db::new_test_connection;
    use proton_mail_test_utils::{conversation, label, rid, search::remote_counterpart};
    use std::sync::LazyLock;
    use test_case::test_case;

    #[derive(Debug, Clone, PartialEq)]
    enum ExpectedMoveAction {
        SystemFolder(ExpectedSystemFolder),
        CustomFolder(ExpectedCustomFolder),
    }

    impl ExpectedMoveAction {
        async fn new(action: MoveAction, tx: &Tether) -> Self {
            match action {
                MoveAction::SystemFolder(_) => {
                    ExpectedMoveAction::SystemFolder(ExpectedSystemFolder::new(action, tx).await)
                }
                MoveAction::CustomFolder(_) => {
                    ExpectedMoveAction::CustomFolder(ExpectedCustomFolder::new(action, tx).await)
                }
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ExpectedSystemFolder {
        label_id: LabelId,
        name: MovableSystemFolder,
        is_selected: Option<bool>,
    }

    impl ExpectedSystemFolder {
        async fn new(action: MoveAction, tx: &Tether) -> Self {
            match action {
                MoveAction::SystemFolder(action) => ExpectedSystemFolder {
                    label_id: remote_counterpart::<Label>(action.local_id, tx)
                        .await
                        .into(),
                    name: action.name,
                    is_selected: action.is_selected,
                },
                _ => panic!("ExpectedSystemFolder::new called with non-SystemFolder action"),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ExpectedCustomFolder {
        label_id: LabelId,
        name: String,
        is_selected: Option<bool>,
        children: Vec<ExpectedCustomFolder>,
    }

    impl ExpectedCustomFolder {
        async fn new(action: MoveAction, tx: &Tether) -> Self {
            match action {
                MoveAction::CustomFolder(action) => ExpectedCustomFolder {
                    label_id: action
                        .local_id
                        .counterpart::<Label, _>(tx)
                        .await
                        .unwrap()
                        .unwrap()
                        .into(),
                    name: action.name,
                    is_selected: action.is_selected,
                    children: stream::iter(action.children)
                        .then(|child| async move {
                            Box::pin(ExpectedCustomFolder::new(
                                MoveAction::CustomFolder(child),
                                tx,
                            ))
                            .await
                        })
                        .collect::<Vec<_>>()
                        .await,
                },
                _ => panic!("ExpectedCustomFolder::new called with non-CustomFolder action"),
            }
        }
    }

    struct ConversationWithLabels {
        conversation: Conversation,
        labels: Vec<Label>,
    }

    static INBOX: LazyLock<Label> = LazyLock::new(
        || label!(label_type: LabelType::System, remote_id: rid!(LabelId::inbox()), name: "Inbox".to_owned(), color: LabelColor::black()),
    );

    static OUTBOX: LazyLock<Label> = LazyLock::new(
        || label!(label_type: LabelType::System, remote_id: rid!(LabelId::outbox()), name: "Outbox".to_owned(), color: LabelColor::black()),
    );

    static STARRED: LazyLock<Label> = LazyLock::new(
        || label!(label_type: LabelType::System, remote_id: rid!(LabelId::starred()), name: "Starred".to_owned(), color: LabelColor::black()),
    );

    static CUSTOM_FOLDER: LazyLock<Label> = LazyLock::new(
        || label!(label_type: LabelType::Folder, remote_id: rid!("1234"), name: "My custom folder".to_owned(), color: LabelColor::purple()),
    );

    #[test_case(&INBOX, vec![], vec![], Err(AppError::EmptyListOfConversations); "TEST1: empty")]
    #[test_case(
        &INBOX,
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![INBOX.clone()] },
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_2")), labels: vec![INBOX.clone()] },
        ],
        vec![
            label!(remote_id: rid!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
            label!(remote_id: rid!("label2"), label_type: LabelType::Folder, name: "label2".to_string()),
        ],
        Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "label1".into(),
                name: "label1".into(),
                is_selected: Some(false),
                children: vec![],
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "label2".into(),
                name: "label2".into(),
                is_selected: Some(false),
                children: vec![]
            }),
        ]); "TEST2: conversations without labels")]
    #[test_case(
        &INBOX,
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![INBOX.clone()] },
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_2")), labels: vec![label!(remote_id: rid!("label2"), label_type: LabelType::Folder, name: "label2".to_string())] },
        ],
        vec![
            label!(remote_id: rid!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
        ],
        Err(AppError::ConversationDoesNotHaveLabel(2.into(), "Inbox".to_string()));
        "TEST3: One conversation in inbox, other in folder")]
    #[test_case(
        &STARRED,
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![STARRED.clone(), OUTBOX.clone()] },
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_2")), labels: vec![STARRED.clone(), INBOX.clone()] },
        ],
        vec![],
        Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Inbox.label_id(),
                name: MovableSystemFolder::Inbox,
                is_selected: None,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
                is_selected: Some(false),
            }),
        ]); "TEST4: One conversation in Inbox, other in Outbox when view is STARRED")]
    #[test_case(
        &CUSTOM_FOLDER,
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![CUSTOM_FOLDER.clone()] },
        ],
        vec![
            label!(remote_id: rid!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
            CUSTOM_FOLDER.clone(),
        ],
        Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Inbox.label_id(),
                name: MovableSystemFolder::Inbox,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "label1".into(),
                name: "label1".into(),
                is_selected: Some(false),
                children: vec![]
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "1234".into(),
                name: "My custom folder".into(),
                is_selected: Some(true),
                children: vec![],
            }),
        ]); "TEST5: Conversation in custom folder, when viewed from custom folder")]
    #[test_case(
        &label!(
            remote_id: rid!("folder2"),
            remote_parent_id: rid!("folder1"),
            name: "folder2".to_string(),
            label_type: LabelType::Folder
        ),
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![
                label!(
                    remote_id: rid!("folder2"),
                    remote_parent_id: rid!("folder1"),
                    name: "folder2".to_string(),
                    label_type: LabelType::Folder
                )
            ] },
        ],
        vec![
            label!(
                remote_id: rid!("folder1"),
                name: "folder1".to_string(),
                label_type: LabelType::Folder
            ),
            label!(
                remote_id: rid!("folder2"),
                remote_parent_id: rid!("folder1"),
                name: "folder2".to_string(),
                label_type: LabelType::Folder
            ),
            label!(
                remote_id: rid!("folder3"),
                remote_parent_id: rid!("folder2"),
                name: "folder3".to_string(),
                label_type: LabelType::Folder
            ),
            label!(
                remote_id: rid!("folder4"),
                remote_parent_id: rid!("folder3"),
                name: "folder4".to_string(),
                label_type: LabelType::Folder
            )
        ],
        Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Inbox.label_id(),
                name: MovableSystemFolder::Inbox,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
                is_selected: Some(false),
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "folder1".into(),
                name: "folder1".into(),
                is_selected: Some(false),
                children: vec![
                    ExpectedCustomFolder {
                        label_id: "folder2".into(),
                        name: "folder2".into(),
                        is_selected: Some(true),
                        children: vec![
                            ExpectedCustomFolder {
                                label_id: "folder3".into(),
                                name: "folder3".into(),
                                is_selected: Some(false),
                                children: vec![
                                    ExpectedCustomFolder {
                                        label_id: "folder4".into(),
                                        name: "folder4".into(),
                                        is_selected: Some(false),
                                        children: vec![]
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }),
        ]); "TEST6: Message in nested custom folder")]
    #[tokio::test]
    async fn test_move_to_actions(
        view: &Label,
        conversations: Vec<ConversationWithLabels>,
        labels: Vec<Label>,
        expected: Result<&[ExpectedMoveAction], AppError>,
    ) {
        let stash = new_test_connection().await;
        let tx = stash.connection();
        let fun_tx = || tx.clone();

        let mut settings = MailSettings::default();
        settings.save_using(&tx).await.unwrap();

        for mut label in labels {
            label.save_using(&tx).await.expect("failed to create label");
        }

        let mut conversation_ids = vec![];

        for ConversationWithLabels {
            mut conversation,
            labels: message_labels,
        } in conversations
        {
            conversation
                .save_using(&tx)
                .await
                .expect("failed to create conversation");

            conversation_ids.push(conversation.local_id.unwrap());

            for mut label in message_labels {
                label.save_using(&tx).await.expect("failed to create label");

                let label_id = label.local_id.unwrap();
                let ids = vec![conversation.local_id.unwrap()];

                Conversation::apply_label(label_id, ids, &tx).await.unwrap();
            }
        }

        let view = Label::find_by_id(view.remote_id.clone().unwrap().into_inner(), &tx)
            .await
            .unwrap()
            .unwrap();

        let result = Conversation::available_move_to_actions(view, conversation_ids, &tx).await;

        match result {
            Ok(actual) => {
                let actual = stream::iter(actual.into_iter())
                    .then(|action| async move { ExpectedMoveAction::new(action, &fun_tx()).await })
                    .collect::<Vec<_>>()
                    .await;

                assert_eq!(actual, expected.unwrap());
            }
            Err(err) => {
                assert_eq!(err.to_string(), expected.unwrap_err().to_string());
            }
        }
    }

    #[tokio::test]
    async fn to_remove() {
        test_move_to_actions(
            &INBOX,
                             vec![
                                 ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_1")), labels: vec![INBOX.clone()] },
                                 ConversationWithLabels { conversation: conversation!(remote_id: rid!("conversation_2")), labels: vec![INBOX.clone()] },
                             ],
                             vec![
                                 label!(remote_id: rid!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
                                 label!(remote_id: rid!("label2"), label_type: LabelType::Folder, name: "label2".to_string()),
                             ],
                             Ok(&[
                                 ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                                     label_id: SystemLabel::Archive.label_id(),
                                     name: MovableSystemFolder::Archive,
                                     is_selected: Some(false),
                                 }),
                                 ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                                     label_id: SystemLabel::Spam.label_id(),
                                     name: MovableSystemFolder::Spam,
                                     is_selected: Some(false),
                                 }),
                                 ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                                     label_id: SystemLabel::Trash.label_id(),
                                     name: MovableSystemFolder::Trash,
                                     is_selected: Some(false),
                                 }),
                                 ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                                     label_id: "label1".into(),
                                     name: "label1".into(),
                                     is_selected: Some(false),
                                     children: vec![],
                                 }),
                                 ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                                     label_id: "label2".into(),
                                     name: "label2".into(),
                                     is_selected: Some(false),
                                     children: vec![]
                                 }),
                             ])).await
    }
}

#[tokio::test]
async fn test_conversation_create_no_labels() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    create_labels(&tx).await;
    let conv = test_conversation(vec![], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.set_stash(&stash);
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conversation, local_conversation);
}

#[tokio::test]
async fn test_conversation_has_messages_flag() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    create_labels(&tx).await;
    let conv = test_conversation(vec![], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.set_stash(&stash);
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");

    let db_conv = Conversation::load(local_conversation.local_id.unwrap(), &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conv.num_messages, 10);
}

#[tokio::test]
async fn test_unknown_conversation_messages_returns_error() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    let id = 1024;
    assert_eq!(
        Message::find("WHERE local_conversation_id = ?", params![id], &stash, None)
            .await
            .expect("failed to get messages"),
        vec![]
    );
}

#[tokio::test]
async fn test_conversation_create_starred() {
    let conv_label = ApiConversationLabel {
        id: LabelId::starred().into(),
        context_num_unread: 0,
        context_num_messages: 0,
        context_time: 0,
        context_size: 0,
        context_num_attachments: 0,
        context_expiration_time: 0,
        context_snooze_time: 0,
    };
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    stash.execute("DELETE FROM labels", vec![]).await.unwrap();
    create_address(&tx).await;
    create_labels(&tx).await;
    test_starred_label().save_using(&tx).await.unwrap();

    // Add starred label, should gain starred attribute.
    let conv = test_conversation(vec![conv_label.clone()], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.set_stash(&stash);
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    {
        let db_conversation = Conversation::load(id, &stash)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        let mut local_conversation = Conversation::from(conv.clone());
        local_conversation.set_stash(&stash);
        local_conversation.row_id = Some(1);
        local_conversation.local_id = Some(1.into());
        local_conversation.labels[0].local_id = Some(1.into());
        local_conversation.labels[0].local_conversation_id = Some(1.into());
        local_conversation.labels[0].set_stash(&stash);
        local_conversation.labels[0].row_id = Some(1);
        local_conversation.labels[0].local_label_id = db_conversation.labels[0].local_label_id;

        assert_eq!(db_conversation, local_conversation);
        assert!(local_conversation.is_starred());
        assert!(db_conversation.is_starred());
    }
    {
        let db_conversation = Conversation::load(id, &stash)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        let mut local_conversation = Conversation::load(id, &stash)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        local_conversation.labels = vec![ConversationLabel {
            local_id: None,
            local_conversation_id: local_conversation.local_id,
            local_label_id: db_conversation.labels[0].local_label_id,
            remote_label_id: LabelId::starred().into(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
            deleted: false,
            row_id: None,
            stash: Some(stash.clone()),
        }];
        local_conversation
            .save_using(&tx)
            .await
            .expect("failed to update conversation");

        assert_eq!(local_conversation, db_conversation);
        assert!(local_conversation.is_starred());
        assert!(db_conversation.is_starred());
    }

    // Remove starred label, should lose starred attribute.
    let mut local_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    local_conversation.labels = vec![];
    local_conversation.set_stash(&stash);
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();
    {
        let db_conversation = Conversation::load(id, &stash)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        assert_eq!(db_conversation, local_conversation);
        assert!(!local_conversation.is_starred());
        assert!(!db_conversation.is_starred());
    }
}

#[tokio::test]
async fn test_conversation_create_with_labels() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    let _local_label_ids = create_labels(&tx).await;
    let conv = test_conversation(
        vec![
            ApiConversationLabel {
                id: MY_LABEL_ID1.clone(),
                context_num_unread: 1,
                context_num_messages: 2,
                context_time: 3,
                context_size: 4,
                context_num_attachments: 5,
                context_expiration_time: 6,
                context_snooze_time: 21,
            },
            ApiConversationLabel {
                id: MY_LABEL_ID2.clone(),
                context_num_unread: 7,
                context_num_messages: 8,
                context_time: 9,
                context_size: 10,
                context_num_attachments: 11,
                context_expiration_time: 12,
                context_snooze_time: 31,
            },
        ],
        vec![],
    );
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.labels = vec![ConversationLabel {
        local_id: None,
        local_conversation_id: None,
        local_label_id: Some(1.into()),
        remote_label_id: LabelId::starred().into(),
        context_num_unread: 0,
        context_num_messages: 0,
        context_time: 0,
        context_size: 0,
        context_num_attachments: 0,
        context_expiration_time: 0,
        context_snooze_time: 0,
        deleted: false,
        row_id: None,
        stash: Some(stash.clone()),
    }];
    local_conversation.set_stash(&stash);
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(local_conversation, db_conversation);
}

#[tokio::test]
async fn test_conversation_create_with_attachment() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    create_labels(&tx).await;
    let conv = test_conversation(
        vec![],
        vec![ApiAttachmentMetadata {
            id: MY_ATTACHMENT_ID.clone(),
            size: 4098,
            name: "My Attachment.pdf".to_owned(),
            mime_type: attachment::MimeType::application_pdf().to_string(),
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.set_stash(&stash);
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    assert_eq!(local_conversation.attachments_metadata.len(), 1);

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conversation.attachments_metadata.len(), 1);

    // Patch local id.
    local_conversation.attachments_metadata[0].local_id =
        RemoteId::from(conv.attachments_metadata[0].id.clone())
            .counterpart::<Attachment, _>(&stash)
            .await
            .unwrap();

    assert_eq!(
        db_conversation.attachments_metadata[0],
        local_conversation.attachments_metadata[0],
    );
}

#[tokio::test]
async fn test_conversation_create_with_attachment_and_label() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    create_labels(&tx).await;
    let conv = test_conversation(
        vec![ApiConversationLabel {
            id: MY_LABEL_ID1.clone(),
            context_num_unread: 1,
            context_num_messages: 2,
            context_time: 3,
            context_size: 4,
            context_num_attachments: 5,
            context_expiration_time: 6,
            context_snooze_time: 7,
        }],
        vec![ApiAttachmentMetadata {
            id: MY_ATTACHMENT_ID.clone(),
            size: 4098,
            name: "My Attachment.pdf".to_owned(),
            mime_type: attachment::MimeType::application_pdf().to_string(),
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation = Conversation::from(conv.clone());
    local_conversation.set_stash(&stash);
    local_conversation
        .save()
        .await
        .expect("failed to create conversation");
    let id = local_conversation.local_id.unwrap();

    assert_eq!(local_conversation.attachments_metadata.len(), 1);

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Patch local id.
    local_conversation.attachments_metadata[0].local_id =
        RemoteId::from(conv.attachments_metadata[0].id.clone())
            .counterpart::<Attachment, _>(&stash)
            .await
            .unwrap();

    assert_eq!(db_conversation.attachments_metadata.len(), 1);
    assert_eq!(
        db_conversation.attachments_metadata[0],
        local_conversation.attachments_metadata[0],
    );
}

#[tokio::test]
async fn test_conversation_update() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    create_address(&tx).await;
    let _local_label_ids = create_labels(&tx).await;
    let conv = test_conversation(
        vec![ApiConversationLabel {
            id: MY_LABEL_ID2.clone(),
            context_num_unread: 7,
            context_num_messages: 8,
            context_time: 9,
            context_size: 10,
            context_num_attachments: 11,
            context_expiration_time: 12,
            context_snooze_time: 21,
        }],
        vec![ApiAttachmentMetadata {
            id: ApiRemoteId::from("ATTACHMENT2"),
            size: 224515,
            name: "Attachment.json".to_owned(),
            mime_type: attachment::MimeType::application_json().to_string(),
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation1 = Conversation::from(conv.clone());
    local_conversation1.set_stash(&stash);
    local_conversation1
        .save()
        .await
        .expect("failed to create conversation");
    let conv_update = test_conversation(
        vec![ApiConversationLabel {
            id: MY_LABEL_ID1.clone(),
            context_num_unread: 1,
            context_num_messages: 2,
            context_time: 3,
            context_size: 4,
            context_num_attachments: 5,
            context_expiration_time: 6,
            context_snooze_time: 7,
        }],
        vec![ApiAttachmentMetadata {
            id: MY_ATTACHMENT_ID.clone(),
            size: 4098,
            name: "My Attachment.pdf".to_owned(),
            mime_type: attachment::MimeType::application_pdf().to_string(),
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation2 = Conversation::from(conv_update.clone());
    local_conversation2.labels = vec![
        ConversationLabel {
            local_id: None,
            local_conversation_id: local_conversation2.local_id,
            local_label_id: None,
            remote_label_id: LabelId::starred().into(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
            deleted: false,
            row_id: None,
            stash: Some(stash.clone()),
        },
        ConversationLabel {
            local_id: None,
            local_conversation_id: local_conversation2.local_id,
            local_label_id: None,
            remote_label_id: LabelId::starred().into(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
            deleted: false,
            row_id: None,
            stash: Some(stash.clone()),
        },
    ];
    local_conversation2.set_stash(&stash);
    local_conversation2.local_id = local_conversation1.local_id;
    local_conversation2.row_id = local_conversation1.row_id;
    local_conversation2
        .save()
        .await
        .expect("failed to update conversation");
    let id = local_conversation2.local_id.unwrap();

    assert_eq!(local_conversation2.attachments_metadata.len(), 1);
    // Patch local id.
    local_conversation2.attachments_metadata[0].local_id =
        RemoteId::from(conv_update.attachments_metadata[0].id.clone())
            .counterpart::<Attachment, _>(&stash)
            .await
            .unwrap();
    local_conversation2.labels.remove(1);

    let db_conversation = Conversation::load(id, &stash)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conversation, local_conversation2);
}

#[tokio::test]
async fn test_conversation_undelete_all_mail() {
    // Same as test_conversation_delete, but undoing the deletions should restore all the state
    // back to the initial values.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;
    let all_mail_label = Label::find_by_id(RemoteId::from(LabelId::all_mail()), tx.stash())
        .await
        .unwrap()
        .unwrap()
        .local_id
        .unwrap();

    let local_conv_id1 = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_conv_id2 = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();
    Conversation::mark_deleted(all_mail_label, vec![local_conv_id1, local_conv_id2], &tx)
        .await
        .expect("failed to mark as deleted");

    Conversation::mark_undeleted(all_mail_label, vec![local_conv_id1, local_conv_id2], &tx)
        .await
        .expect("failed to mark conversations as undeleted");

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_delete_all_mail() {
    // Simulate conversation delete from all mail, all messages for the conversation a
    // are deleted.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;
    let all_mail_label = SystemLabel::AllMail.local_id(&tx).await.unwrap().unwrap();

    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    Conversation::mark_deleted(all_mail_label, vec![local_conv_id], &tx)
        .await
        .expect("failed to mark as deleted");

    let db_conversation = Conversation::find_first(
        "WHERE local_id = ? AND deleted = 0",
        params![local_conv_id],
        tx.stash(),
    )
    .await
    .expect("failed to get conversation");
    assert!(db_conversation.is_none());

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1,);
            assert_eq!(label_counts.total, start_label_counts.total - 1,);
        }
        // Check conversation label2 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1,);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID1.clone().into(),
            );
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total - total);
        }
        // Check label2
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID2.clone().into(),
            );
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total - total);
        }
    }

    // Deleting conv2 should reset all counters to 0.
    let local_conv_id = *state_map
        .conversations
        .get(&state.conversations[1].remote_id.clone().unwrap())
        .unwrap();
    Conversation::mark_deleted(all_mail_label, vec![local_conv_id], &tx)
        .await
        .expect("failed to mark conv as deleted");

    for count in Label::all(tx.stash(), None).await.unwrap() {
        assert_eq!(
            count.total_msg, 0,
            "Label {:?} does not have 0 total count",
            count.local_id
        );
        assert_eq!(
            count.unread_msg, 0,
            "Label {:?} does not have 0 unread count",
            count.local_id
        );
        assert_eq!(
            count.total_conv, 0,
            "Label {:?} does not have 0 total count",
            count.local_id
        );
        assert_eq!(
            count.unread_conv, 0,
            "Label {:?} does not have 0 unread count",
            count.local_id
        );
    }
}

#[tokio::test]
async fn test_conversation_delete() {
    // Simulate conversation according to API expectations, only delete conversations in that label.
    // If conversation has messages in other labels, it must still exist.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;
    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();
    Conversation::mark_deleted(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to mark as deleted");

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // No more unread messages
    assert_eq!(db_conversation.num_unread, 1);
    // Should only have one message in other label
    assert_eq!(db_conversation.num_messages, 2);
    assert_eq!(
        db_conversation.size,
        state.messages[0].size + state.messages[3].size
    );
    assert_eq!(
        db_conversation.num_attachments,
        state.messages[0].num_attachments as u64 + state.messages[3].num_attachments as u64
    );

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID1.clone().into(),
            );
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total - total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Deleting conv1 in label 2  should remove all traces of the  conversation
    Conversation::mark_deleted(local_label_id2, vec![local_conv_id], &tx)
        .await
        .expect("failed to mark conv as deleted");

    {
        let db_conversation = Conversation::find_first(
            "WHERE local_id = ? AND deleted = 0",
            params![local_conv_id],
            tx.stash(),
        )
        .await
        .expect("failed to get conversation");
        assert!(db_conversation.is_none());
    }

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, should be empty
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 0);
        }
        // Check conversation label2 values, should be missing one conversation.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 0);
        }
        // Check label2 - should be missing two messages.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total - 2);
        }
    }
}

#[tokio::test]
async fn test_conversation_undelete() {
    // Same as test_conversation_delete, but checks for reverse operations.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();
    Conversation::mark_deleted(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to mark as deleted");
    Conversation::mark_deleted(local_label_id2, vec![local_conv_id], &tx)
        .await
        .expect("failed to mark as deleted");

    Conversation::mark_undeleted(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("Failed to mark as undeleted");
    Conversation::mark_undeleted(local_label_id2, vec![local_conv_id], &tx)
        .await
        .expect("Failed to mark as undeleted");

    let db_conversation = Conversation::load(local_conv_id, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Conversation should match original values.
    {
        let original = &state.conversations[0];
        assert_eq!(db_conversation.num_unread, original.num_unread);
        assert_eq!(db_conversation.num_messages, original.num_messages);
        assert_eq!(db_conversation.size, original.size);
        assert_eq!(db_conversation.num_attachments, original.num_attachments);
    }

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, should match original state.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1 - should match original state.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_counts() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    stash.execute("DELETE FROM labels", vec![]).await.unwrap();
    create_address(&tx).await;
    let labels = create_labels(&tx).await;
    let counts = vec![
        ConversationCount {
            label_id: MY_LABEL_ID1.clone().into(),
            total: 20,
            unread: 4,
        },
        ConversationCount {
            label_id: MY_LABEL_ID2.clone().into(),
            total: 400,
            unread: 124,
        },
    ];

    Label::create_or_update_conversation_counts(counts.clone(), tx.stash())
        .await
        .expect("failed to creat counters");
    let db_labels = Label::all(tx.stash(), None)
        .await
        .expect("failed to get counters");
    let db_counters = db_labels
        .iter()
        .map(|c| ConversationCount {
            label_id: c.remote_id.clone().unwrap(),
            total: c.total_conv,
            unread: c.unread_conv,
        })
        .collect::<Vec<_>>();
    assert!(db_counters.contains(&counts[0]));
    assert!(db_counters.contains(&counts[1]));

    let label_conv_counter = Label::load(labels[0], tx.stash()).await.unwrap().unwrap();
    assert!(db_labels.contains(&label_conv_counter));

    assert_eq!(db_labels.len(), 2);
    assert_eq!(db_labels[0].remote_id, counts[0].label_id.clone().into());
    assert_eq!(db_labels[0].total_conv, counts[0].total);
    assert_eq!(db_labels[0].unread_conv, counts[0].unread);
}

#[tokio::test]
async fn test_conversation_mark_read_no_message_metadata() {
    // Mark conversation as read without message metadata.
    let mut state = new_test_unread_db_state();
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    Conversation::mark_read(std::iter::once(local_conv_id), &tx)
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // No more unread messages
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 4);

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_mark_read() {
    // Mark conversation as read and update all conversation / message counts
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    Conversation::mark_read(std::iter::once(local_conv_id), &tx)
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // No more unread messages
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 4);

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let (unread, _) = message_counts_for_conversation(
                &state.messages,
                state.conversations[0].remote_id.as_ref().unwrap(),
                &MY_LABEL_ID1.clone().into(),
            );
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_mark_unread_no_metadata() {
    // Mark conversation as read and then mark it unread, since we don't have message
    // metadata we should mark the current conversation label only as unread.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    Conversation::mark_read(std::iter::once(local_conv_id), &tx)
        .await
        .unwrap();

    Conversation::mark_unread(local_label_id1, std::iter::once(local_conv_id), &tx)
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 4);

    // Check conversation counts match original values.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        {
            // Label2 should have no unread messages since the message in conv 1 is not the latest.
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_mark_unread() {
    // Mark conversation as read and then mark it unread, only the LATEST message in the
    // conversation should be marked unread.
    //
    // SETUP:
    // Conversation 1 has 4 messages, All unread.
    // 3 are in label1 and 1 in label2
    // The last message in the conversation is of label1
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let state = new_test_unread_db_state();
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone().into()).unwrap();

    // First mark all msgs as unread
    Conversation::mark_read([local_conv_id], &tx).await.unwrap();

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    assert_eq!(db_conversation.num_messages, 4);
    assert_eq!(db_conversation.num_unread, 0);

    // Mark last one as unread
    Conversation::mark_unread(local_label_id1, [local_conv_id], &tx)
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, tx.stash())
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    let messages = Message::find(
        "WHERE local_conversation_id=? 
                AND unread=1",
        params![local_conv_id],
        tx.stash(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(messages.len(), 1);
    let message = &messages[0];

    assert_eq!(&message.label_ids[0], &MY_LABEL_ID1.clone().into());

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);

    // Assert label conversation counts are 0
    // The unread conversation counts should be 0 because the conversation is
    // not fully marked as unread
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tx).await;

        // Check label1
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID1.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map
                .message_counts
                .get(&MY_LABEL_ID2.clone().into())
                .unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_label_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 3);
    assert_eq!(db_conversation.num_attachments, 1);
    assert_eq!(
        db_conversation.size,
        state.messages.iter().fold(0, |x, m| x + m.size)
    );
    assert_eq!(
        db_conversation.expiration_time,
        state
            .messages
            .iter()
            .fold(0, |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tx).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 3);
    }
}

#[tokio::test]
async fn test_conversation_double_label_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation twice and check
    // the changes are not duplicated.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 3);
    assert_eq!(db_conversation.num_attachments, 1);
    assert_eq!(
        db_conversation.size,
        state.messages.iter().fold(0, |x, m| x + m.size)
    );
    assert_eq!(
        db_conversation.expiration_time,
        state
            .messages
            .iter()
            .fold(0, |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tx).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 3);
    }
}

#[tokio::test]
async fn test_conversation_label_partially() {
    // Label conversation with a label where one of the messages already has been labeled
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let mut state = state.clone();
    state.messages[1]
        .label_ids
        .push(MY_LABEL_ID1.clone().into());
    state.conversations[0].labels.push(
        ApiConversationLabel {
            id: MY_LABEL_ID1.clone().into(),
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_messages: 0,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: 0,
            context_time: 0,
        }
        .into(),
    );
    let (state, state_map) = prepare_and_patch_db_state(&tx, state).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 3);
    assert_eq!(db_conversation.num_attachments, 1);
    assert_eq!(
        db_conversation.size,
        state.messages.iter().fold(0, |x, m| x + m.size)
    );
    assert_eq!(
        db_conversation.expiration_time,
        state
            .messages
            .iter()
            .fold(0, |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tx).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 3);
    }
}

#[tokio::test]
async fn test_conversation_label_without_message_metadata() {
    // Label a conversation with a label that was never assigned without having any message metadata
    // present.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 0);
    assert_eq!(db_conversation.num_attachments, 0);
    assert_eq!(db_conversation.size, 0);
    assert_eq!(db_conversation.time, 0);
    assert_eq!(db_conversation.expiration_time, 0);
    assert_eq!(db_conversation.snooze_time, 0);

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            // unread is 0 due to lack of messages.
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 1);
        }
    }
}

#[tokio::test]
async fn test_conversation_double_label_without_message_metadata() {
    // Label a conversation with a label that was never assigned without having any message metadata
    // present 2 times and check the data is not duplicated.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 0);
    assert_eq!(db_conversation.num_attachments, 0);
    assert_eq!(db_conversation.size, 0);
    assert_eq!(db_conversation.time, 0);
    assert_eq!(db_conversation.expiration_time, 0);
    assert_eq!(db_conversation.snooze_time, 0);

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            // unread is 0 due to lack of messages.
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 1);
        }
    }
}

#[tokio::test]
async fn test_conversation_label_without_metadata_uses_information_from_other_labels() {
    // Check that when we label a conversation without message metadata, we
    // grab the maximum value of the other labels this conversation belongs to.
    // There is a fallback to 0 values if no such thing exists. In production
    // conversation will always be assigned to the "All Mail".
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state_label_with_existing_labels();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tx)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    let conv_label = &state.conversations[0].labels[0];
    assert_eq!(db_conversation.num_unread, conv_label.context_num_unread);
    assert_eq!(
        db_conversation.num_messages,
        conv_label.context_num_messages
    );
    assert_eq!(
        db_conversation.num_attachments,
        conv_label.context_num_attachments
    );
    assert_eq!(db_conversation.size, conv_label.context_size);
    assert_eq!(
        db_conversation.expiration_time,
        conv_label.context_expiration_time
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            // unread is 0 due to lack of messages.
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 1);
        }
    }
}

#[tokio::test]
async fn test_conversation_unlabel_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");
    Conversation::remove_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to unlabel");

    assert!(
        ContextualConversation::load(local_conv_id, local_label_id1, &tx)
            .await
            .expect("failed to get conversation")
            .is_none()
    );

    // Check conversation counts should be 0
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 0);
    }

    // Check message counts should be 0
    {
        let message_counts = msg_counts_as_map(&tx).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 0);
    }
}

#[tokio::test]
async fn test_conversation_unlabel_without_message_metadata() {
    // Label and then unlabel a conversation with a label that was never assigned without having any message metadata
    // present.
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");
    Conversation::remove_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    assert!(
        ContextualConversation::load(local_conv_id, local_label_id1, &tx)
            .await
            .expect("failed to get conversation")
            .is_none()
    );

    // Check conversation counts should be 0
    {
        let conv_counts = conv_counts_as_map(&tx).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 0);
    }
}

#[tokio::test]
async fn test_conversation_expiration() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;
    let tx = &AgnosticInterface::from(tx);

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();

    dbg!(state.conversations.len());

    // Delete all expired, no matches
    let res = Conversation::delete_expired(tx).await.unwrap();
    assert_eq!(res, 0);

    let cv = Conversation::load(local_conv_id, tx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cv.expiration_time, 0);
    assert_eq!(cv.deleted, false);

    // Load a conversation
    Conversation::set_expiration_time_in(local_conv_id, -1000, tx)
        .await
        .unwrap();

    // Delete all expired
    let res = Conversation::delete_expired(tx).await.unwrap();

    assert_eq!(res, 1);

    // Check if it's deleted
    let cv = Conversation::load(local_conv_id, tx)
        .await
        .unwrap()
        .unwrap();

    // Check that all messages are deleted too
    let messages = cv.load_messages(tx).await.unwrap();
    for message in messages {
        assert_eq!(message.deleted, true);
    }

    assert_eq!(cv.deleted, true);
}

#[tokio::test]
async fn test_conversation_watcher() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();
    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    let (_, watch_result) = ContextualConversation::watch_in_label(local_label_id1, &tx)
        .await
        .unwrap();

    tokio::spawn(async move {
        //bypass model to only execute exactly 2 queries.
        tx.execute("UPDATE conversation_labels SET context_num_unread=? WHERE local_label_id=? AND local_conversation_id=?",
                   params![30, local_label_id1, local_conv_id],
        ).await.unwrap();
        tx.execute(
            "UPDATE conversations SET num_unread=? WHERE local_id=?",
            params![10, local_conv_id],
        )
        .await
        .unwrap();
    });

    // first update when modifying label
    watch_result.recv_async().await.unwrap();
    // second update when modifying conversation
    watch_result.recv_async().await.unwrap();
}

#[tokio::test]
async fn test_contextual_conversation_messages() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state_and_skip(&tx, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();

    let watch_result = ContextualConversation::watch_conversation_and_messages(local_conv_id, &tx)
        .await
        .unwrap();

    Conversation::apply_label(local_label_id1, vec![local_conv_id], &tx)
        .await
        .expect("failed to label");

    watch_result.recv_async().await.unwrap();
}
