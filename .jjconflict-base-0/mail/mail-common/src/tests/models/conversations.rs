use std::sync::LazyLock;

use super::*;
use crate::datatypes::{
    ContextualConversation, MessageFlags, MessageSender, MovableSystemFolder, SystemLabelId,
    attachment,
};
use crate::label;
use crate::models::{Attachment, Conversation, ConversationLabel, MailSettings, Message};
use crate::test_utils::db::new_test_connection_file;
use crate::test_utils::db_states::{
    new_conversation_snooze_db_state, new_test_delete_db_state, new_test_label_db_state,
    new_test_label_db_state_label_with_existing_labels, new_test_label_expiration_db_state,
    new_test_unread_db_state, new_test_unread_db_state_unread_label_in_folder,
};
use crate::test_utils::search::{
    MY_ATTACHMENT_ID, MY_LABEL_ID1, MY_LABEL_ID2, create_labels, test_conversation, test_label1,
    test_starred_label,
};
use crate::test_utils::utils::{
    TestDBState, conv_counts_as_map, create_address, message_counts_for_conversation,
    msg_counts_as_map, prepare_and_patch_db_state, prepare_and_patch_db_state_and_skip,
    prepare_db_state_core,
};
use pretty_assertions::assert_eq;
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LabelColor, LabelType};
use proton_core_common::models::Label;
use proton_mail_api::services::proton::common::AttachmentId;
use proton_mail_api::services::proton::response_data::{
    AttachmentMetadata as ApiAttachmentMetadata, ConversationLabel as ApiConversationLabel,
    Disposition as ApiDisposition,
};
use stash::orm::Model;
use stash::params;
use test_case::test_case;

mod first_unread_message {
    use std::sync::LazyLock;

    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    static STARRED: LazyLock<Label> =
        LazyLock::new(|| new_label(LabelType::System, Some(LabelId::starred().clone())));

    static LABEL: LazyLock<Label> =
        LazyLock::new(|| new_label(LabelType::Label, Some("label".into())));

    static FOLDER: LazyLock<Label> =
        LazyLock::new(|| new_label(LabelType::Folder, Some("folder".into())));

    static INBOX: LazyLock<Label> =
        LazyLock::new(|| new_label(LabelType::System, Some(LabelId::inbox().clone())));

    static DRAFTS: LazyLock<Label> =
        LazyLock::new(|| new_label(LabelType::System, Some(LabelId::drafts().clone()))); // There is no conversations in drafts - this is theoretical case

    static ALL_LABELS: LazyLock<Vec<&'static Label>> =
        LazyLock::new(|| vec![&STARRED, &LABEL, &FOLDER, &INBOX, &DRAFTS]);

    static MOVED_CONV_LABELS: LazyLock<Vec<&'static Label>> =
        LazyLock::new(|| vec![&STARRED, &LABEL, &FOLDER]);

    static INBOX_AND_DRAFTS_LABELS: LazyLock<Vec<&'static Label>> =
        LazyLock::new(|| vec![&INBOX, &DRAFTS]);

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
    ], Some(2.into()); "TEST26 - different view labels"
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
    #[test_case(
    &[&FOLDER], &[
        (MessageFlags::RECEIVED, false, &[&INBOX]),
        (MessageFlags::RECEIVED, false, &[&FOLDER]),
        (MessageFlags::RECEIVED, false, &[&INBOX]),
    ], Some(1.into()); "TEST29 - different view labels in custom folder"
    )]
    fn first_unread_message(
        labels: &[&Label],
        messages: &[(MessageFlags, bool, &[&Label])],
        expected_id: Option<LocalMessageId>,
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
        id: LocalMessageId,
        flags: MessageFlags,
        unread: bool,
        labels: &[&Label],
    ) -> Message {
        let label_ids = labels
            .iter()
            .map(|label| label.remote_id.clone().unwrap())
            .collect();

        // TODO: apply_labels
        Message {
            local_id: Some(id),
            unread,
            sender: MessageSender {
                address: String::new().into(),
                bimi_selector: None,
                display_sender_image: false,
                is_proton: false,
                is_simple_login: false,
                name: String::new().into(),
            },
            flags,
            label_ids,
            ..Message::test_default()
        }
    }

    fn new_label(label_type: LabelType, rid: Option<LabelId>) -> Label {
        label!(label_type, remote_id: rid)
    }
}

mod available_move_to_actions {
    use super::*;
    use crate::test_utils::db::new_test_connection;
    use crate::{conv_id, conversation, label, lbl_id};
    use futures::stream::{self, StreamExt};
    use pretty_assertions::assert_eq;
    use proton_core_common::datatypes::{LabelColor, LabelType, SystemLabel};
    use stash::stash::Tether;
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
    }

    impl ExpectedSystemFolder {
        async fn new(action: MoveAction, tx: &Tether) -> Self {
            match action {
                MoveAction::SystemFolder(action) => ExpectedSystemFolder {
                    label_id: Label::local_id_counterpart(action.local_id, tx)
                        .await
                        .unwrap()
                        .unwrap(),
                    name: action.name,
                },
                _ => panic!("ExpectedSystemFolder::new called with non-SystemFolder action"),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ExpectedCustomFolder {
        label_id: LabelId,
        name: String,
        children: Vec<ExpectedCustomFolder>,
    }

    impl ExpectedCustomFolder {
        async fn new(action: MoveAction, tx: &Tether) -> Self {
            match action {
                MoveAction::CustomFolder(action) => ExpectedCustomFolder {
                    label_id: Label::local_id_counterpart(action.local_id, tx)
                        .await
                        .unwrap()
                        .unwrap(),
                    name: action.name,
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
        || label!(label_type: LabelType::System, remote_id: lbl_id!(LabelId::inbox()), name: "Inbox".to_owned(), color: LabelColor::black()),
    );

    static OUTBOX: LazyLock<Label> = LazyLock::new(
        || label!(label_type: LabelType::System, remote_id: lbl_id!(LabelId::outbox()), name: "Outbox".to_owned(), color: LabelColor::black()),
    );

    static STARRED: LazyLock<Label> = LazyLock::new(
        || label!(label_type: LabelType::System, remote_id: lbl_id!(LabelId::starred()), name: "Starred".to_owned(), color: LabelColor::black()),
    );

    static CUSTOM_FOLDER: LazyLock<Label> = LazyLock::new(
        || label!(label_type: LabelType::Folder, remote_id: lbl_id!("1234"), name: "My custom folder".to_owned(), color: LabelColor::purple()),
    );

    #[test_case(&INBOX, vec![], vec![], Err(AppError::EmptyListOfConversations); "TEST1: empty")]
    #[test_case(
        &INBOX,
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_1")), labels: vec![INBOX.clone()] },
            ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_2")), labels: vec![INBOX.clone()] },
        ],
        vec![
            label!(remote_id: lbl_id!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
            label!(remote_id: lbl_id!("label2"), label_type: LabelType::Folder, name: "label2".to_string()),
        ],
        Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "label1".into(),
                name: "label1".into(),
                children: vec![],
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "label2".into(),
                name: "label2".into(),
                children: vec![]
            }),
        ]); "TEST2: conversations without labels")]
    #[test_case(
        &INBOX,
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_1")), labels: vec![INBOX.clone()] },
            ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_2")), labels: vec![label!(remote_id: lbl_id!("label2"), label_type: LabelType::Folder, name: "label2".to_string())] },
        ],
        vec![
            label!(remote_id: lbl_id!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
        ],
        Err(AppError::ConversationDoesNotHaveLabel(2.into(), "Inbox".to_string()));
        "TEST3: One conversation in inbox, other in folder")]
    #[test_case(
        &STARRED,
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_1")), labels: vec![STARRED.clone(), OUTBOX.clone()] },
            ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_2")), labels: vec![STARRED.clone(), INBOX.clone()] },
        ],
        vec![],
        Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Inbox.label_id(),
                name: MovableSystemFolder::Inbox,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
            }),
        ]); "TEST4: One conversation in Inbox, other in Outbox when view is STARRED")]
    #[test_case(
        &CUSTOM_FOLDER,
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_1")), labels: vec![CUSTOM_FOLDER.clone()] },
        ],
        vec![
            label!(remote_id: lbl_id!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
            CUSTOM_FOLDER.clone(),
        ],
        Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Inbox.label_id(),
                name: MovableSystemFolder::Inbox,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "label1".into(),
                name: "label1".into(),
                children: vec![]
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "1234".into(),
                name: "My custom folder".into(),
                children: vec![],
            }),
        ]); "TEST5: Conversation in custom folder, when viewed from custom folder")]
    #[test_case(
        &label!(
            remote_id: lbl_id!("folder2"),
            remote_parent_id: lbl_id!("folder1"),
            name: "folder2".to_string(),
            label_type: LabelType::Folder
        ),
        vec![
            ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_1")), labels: vec![
                label!(
                    remote_id: lbl_id!("folder2"),
                    remote_parent_id: lbl_id!("folder1"),
                    name: "folder2".to_string(),
                    label_type: LabelType::Folder
                )
            ] },
        ],
        vec![
            label!(
                remote_id: lbl_id!("folder1"),
                name: "folder1".to_string(),
                label_type: LabelType::Folder
            ),
            label!(
                remote_id: lbl_id!("folder2"),
                remote_parent_id: lbl_id!("folder1"),
                name: "folder2".to_string(),
                label_type: LabelType::Folder
            ),
            label!(
                remote_id: lbl_id!("folder3"),
                remote_parent_id: lbl_id!("folder2"),
                name: "folder3".to_string(),
                label_type: LabelType::Folder
            ),
            label!(
                remote_id: lbl_id!("folder4"),
                remote_parent_id: lbl_id!("folder3"),
                name: "folder4".to_string(),
                label_type: LabelType::Folder
            )
        ],
        Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Inbox.label_id(),
                name: MovableSystemFolder::Inbox,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "folder1".into(),
                name: "folder1".into(),
                children: vec![
                    ExpectedCustomFolder {
                        label_id: "folder2".into(),
                        name: "folder2".into(),
                        children: vec![
                            ExpectedCustomFolder {
                                label_id: "folder3".into(),
                                name: "folder3".into(),
                                children: vec![
                                    ExpectedCustomFolder {
                                        label_id: "folder4".into(),
                                        name: "folder4".into(),
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
        let mut conn = stash.connection().await.unwrap();

        let mut settings = MailSettings::default();
        let mut conversation_ids = vec![];
        conn.tx::<_, _, StashError>(async |tx| {
            settings.save(tx).await.unwrap();
            for mut label in labels {
                label.save(tx).await.expect("failed to create label");
            }

            for ConversationWithLabels {
                mut conversation,
                labels: message_labels,
            } in conversations
            {
                conversation
                    .save(tx)
                    .await
                    .expect("failed to create conversation");

                conversation_ids.push(conversation.id());

                for mut label in message_labels {
                    label.save(tx).await.expect("failed to create label");

                    let label_id = label.id();

                    ConversationCounter::new(label_id)
                        .save(tx)
                        .await
                        .expect("Failed to create counters");

                    let ids = vec![conversation.id()];

                    Conversation::apply_label_async(label_id, ids, tx)
                        .await
                        .unwrap();
                }
            }
            Ok(())
        })
        .await
        .unwrap();

        let view = Label::find_by_remote_id(view.remote_id.clone().unwrap(), &conn)
            .await
            .unwrap()
            .unwrap();

        let result = Conversation::available_move_to_actions(view, conversation_ids, &conn).await;
        let new_conn = async || stash.connection().await.unwrap();

        match result {
            Ok(actual) => {
                let actual = stream::iter(actual.into_iter())
                    .then(|action| async move {
                        let tether = new_conn().await;
                        ExpectedMoveAction::new(action, &tether).await
                    })
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
                ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_1")), labels: vec![INBOX.clone()] },
                ConversationWithLabels { conversation: conversation!(remote_id: conv_id!("conversation_2")), labels: vec![INBOX.clone()] },
            ],
            vec![
                label!(remote_id: lbl_id!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
                label!(remote_id: lbl_id!("label2"), label_type: LabelType::Folder, name: "label2".to_string()),
            ],
            Ok(&[
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Archive.label_id(),
                name: MovableSystemFolder::Archive,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Spam.label_id(),
                name: MovableSystemFolder::Spam,
            }),
            ExpectedMoveAction::SystemFolder(ExpectedSystemFolder {
                label_id: SystemLabel::Trash.label_id(),
                name: MovableSystemFolder::Trash,
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "label1".into(),
                name: "label1".into(),
                children: vec![],
            }),
            ExpectedMoveAction::CustomFolder(ExpectedCustomFolder {
                label_id: "label2".into(),
                name: "label2".into(),
                children: vec![]
            }),
        ])).await
    }
}

#[tokio::test]
async fn test_conversation_create_no_labels() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    create_address(&mut tether).await;
    create_labels(&mut tether).await;
    let conv = test_conversation(vec![], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();
    let id = local_conversation.id();

    let db_conversation = Conversation::load(id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conversation, local_conversation);
}

#[tokio::test]
async fn test_conversation_has_messages_flag() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    create_address(&mut tether).await;
    create_labels(&mut tether).await;
    let conv = test_conversation(vec![], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();

    let db_conv = Conversation::load(local_conversation.id(), &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conv.num_messages, 10);
}

#[tokio::test]
async fn test_unknown_conversation_messages_returns_error() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    create_address(&mut tether).await;
    let id = 1024;
    assert_eq!(
        Message::find("WHERE local_conversation_id = ?", params![id], &tether,)
            .await
            .expect("failed to get messages"),
        vec![]
    );
}

#[tokio::test]
async fn test_conversation_create_starred() {
    let conv_label = ApiConversationLabel {
        id: LabelId::starred(),
        context_num_unread: 0,
        context_num_messages: 0,
        context_time: 0,
        context_size: 0,
        context_num_attachments: 0,
        context_expiration_time: 0,
        context_snooze_time: 0,
    };
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    tether.execute("DELETE FROM labels", vec![]).await.unwrap();
    create_address(&mut tether).await;
    create_labels(&mut tether).await;
    tether
        .tx::<_, _, StashError>(async |tx| {
            test_starred_label().save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Add starred label, should gain starred attribute.
    let conv = test_conversation(vec![conv_label.clone()], vec![]);
    let mut local_conversation = Conversation::from(conv.clone());
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();
    let id = local_conversation.id();

    {
        let db_conversation = Conversation::load(id, &tether)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        let mut local_conversation = Conversation::from(conv.clone());
        local_conversation.local_id = Some(LocalConversationId::from(1));
        local_conversation.labels[0].local_id = Some(1);
        local_conversation.labels[0].local_conversation_id = Some(1.into());
        local_conversation.labels[0].local_label_id = db_conversation.labels[0].local_label_id;

        assert_eq!(db_conversation, local_conversation);
        assert!(local_conversation.is_starred());
        assert!(db_conversation.is_starred());
    }
    {
        let db_conversation = Conversation::load(id, &tether)
            .await
            .expect("failed to get conversation")
            .expect("should have value");
        let mut local_conversation = Conversation::load(id, &tether)
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
            context_time: 0.into(),
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0.into(),
            context_snooze_time: 0.into(),
            deleted: false,
        }];
        tether
            .tx::<_, _, StashError>(async |tx| {
                local_conversation
                    .save(tx)
                    .await
                    .expect("failed to update conversation");
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(local_conversation, db_conversation);
        assert!(local_conversation.is_starred());
        assert!(db_conversation.is_starred());
    }

    // Remove starred label, should lose starred attribute.
    let mut local_conversation = Conversation::load(id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    local_conversation.labels = vec![];
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();
    let id = local_conversation.id();
    {
        let db_conversation = Conversation::load(id, &tether)
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
    let mut tether = stash.connection().await.unwrap();
    create_address(&mut tether).await;
    let _local_label_ids = create_labels(&mut tether).await;
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
        context_time: 0.into(),
        context_size: 0,
        context_num_attachments: 0,
        context_expiration_time: 0.into(),
        context_snooze_time: 0.into(),
        deleted: false,
    }];
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();
    let id = local_conversation.id();

    let db_conversation = Conversation::load(id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(local_conversation, db_conversation);
}

#[tokio::test]
async fn test_conversation_create_with_attachment() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    create_address(&mut tether).await;
    create_labels(&mut tether).await;
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
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();
    let id = local_conversation.id();

    assert_eq!(local_conversation.attachments_metadata.len(), 1);

    let db_conversation = Conversation::load(id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");
    assert_eq!(db_conversation.attachments_metadata.len(), 1);

    // Patch local id.
    local_conversation.attachments_metadata[0].local_id =
        Attachment::remote_id_counterpart(conv.attachments_metadata[0].id.clone(), &tether)
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
    let mut tether = stash.connection().await.unwrap();
    create_address(&mut tether).await;
    create_labels(&mut tether).await;
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
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();
    let id = local_conversation.id();

    assert_eq!(local_conversation.attachments_metadata.len(), 1);

    let db_conversation = Conversation::load(id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Patch local id.
    local_conversation.attachments_metadata[0].local_id =
        Attachment::remote_id_counterpart(conv.attachments_metadata[0].id.clone(), &tether)
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
    let mut tether = stash.connection().await.unwrap();
    create_address(&mut tether).await;
    let _local_label_ids = create_labels(&mut tether).await;
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
            id: AttachmentId::from("ATTACHMENT2"),
            size: 224515,
            name: "Attachment.json".to_owned(),
            mime_type: attachment::MimeType::application_json().to_string(),
            disposition: ApiDisposition::Attachment,
        }],
    );
    let mut local_conversation1 = Conversation::from(conv.clone());
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation1
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();
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
            context_time: 0.into(),
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0.into(),
            context_snooze_time: 0.into(),
            deleted: false,
        },
        ConversationLabel {
            local_id: None,
            local_conversation_id: local_conversation2.local_id,
            local_label_id: None,
            remote_label_id: LabelId::starred().into(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0.into(),
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0.into(),
            context_snooze_time: 0.into(),
            deleted: false,
        },
    ];
    local_conversation2.local_id = local_conversation1.local_id;
    tether
        .tx::<_, _, StashError>(async |tx| {
            local_conversation2
                .save(tx)
                .await
                .expect("failed to update conversation");
            Ok(())
        })
        .await
        .unwrap();
    let id = local_conversation2.id();

    assert_eq!(local_conversation2.attachments_metadata.len(), 1);
    // Patch local id.
    local_conversation2.attachments_metadata[0].local_id =
        Attachment::remote_id_counterpart(conv_update.attachments_metadata[0].id.clone(), &tether)
            .await
            .unwrap();
    local_conversation2.labels.remove(1);

    let db_conversation = Conversation::load(id, &tether)
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
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;
    let all_mail_label = Label::find_by_remote_id(LabelId::all_mail(), &tether)
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
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::mark_deleted(all_mail_label, vec![local_conv_id1, local_conv_id2], tx)
                .await
                .expect("failed to mark as deleted");

            Conversation::mark_undeleted(all_mail_label, vec![local_conv_id1, local_conv_id2], tx)
                .await
                .expect("failed to mark conversations as undeleted");

            Ok(())
        })
        .await
        .unwrap();

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        // Check conversation label1 values
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tether).await;

        // Check label1
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1.clone()).unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2.clone()).unwrap();
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
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;
    let all_mail_label = SystemLabel::AllMail
        .local_id(&tether)
        .await
        .unwrap()
        .unwrap();

    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::mark_deleted(all_mail_label, vec![local_conv_id], tx)
                .await
                .expect("failed to mark as deleted");
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::find_first(
        "WHERE local_id = ? AND deleted = 0",
        params![local_conv_id],
        &tether,
    )
    .await
    .expect("failed to get conversation");
    assert!(db_conversation.is_none());

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        // Check conversation label1 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1,);
            assert_eq!(label_counts.total, start_label_counts.total - 1,);
        }
        // Check conversation label2 values
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1,);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tether).await;

        // Check label1
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID1.clone(),
            );
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1.clone()).unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total - total);
        }
        // Check label2
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID2.clone(),
            );
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2.clone()).unwrap();
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
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::mark_deleted(all_mail_label, vec![local_conv_id], tx)
                .await
                .expect("failed to mark conv as deleted");
            Ok(())
        })
        .await
        .unwrap();

    let all_counters = MessageCounter::all(&tether).await.expect("no error");
    tracing::error!("ALL COUNTERS {all_counters:?}");

    for count in Label::all(&tether).await.unwrap() {
        tracing::error!("Count {count:?}");
        let counters = LabelWithCounters::load(count.id(), &tether)
            .await
            .expect("no error")
            .expect("counter assigned to the label");
        assert_eq!(
            counters.total_msg, 0,
            "Label {:?} does not have 0 total count",
            count.local_id
        );
        assert_eq!(
            counters.unread_msg, 0,
            "Label {:?} does not have 0 unread count",
            count.local_id
        );
        assert_eq!(
            counters.total_conv, 0,
            "Label {:?} does not have 0 total count",
            count.local_id
        );
        assert_eq!(
            counters.unread_conv, 0,
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
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;
    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1.clone()).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2.clone()).unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::mark_deleted(local_label_id1, vec![local_conv_id], tx)
                .await
                .expect("failed to mark as deleted");
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
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
        let conv_counts = conv_counts_as_map(&tether).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tether).await;

        // Check label1
        {
            let (unread, total) = message_counts_for_conversation(
                &state.messages,
                &state.conversations[0].remote_id.clone().unwrap(),
                &MY_LABEL_ID1,
            );
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total - total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Deleting conv1 in label 2  should remove all traces of the  conversation
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::mark_deleted(local_label_id2, vec![local_conv_id], tx)
                .await
                .expect("failed to mark conv as deleted");
            Ok(())
        })
        .await
        .unwrap();

    {
        let db_conversation = Conversation::find_first(
            "WHERE local_id = ? AND deleted = 0",
            params![local_conv_id],
            &tether,
        )
        .await
        .expect("failed to get conversation");
        assert!(db_conversation.is_none());
    }

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        // Check conversation label1 values, should be empty
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 0);
        }
        // Check conversation label2 values, should be missing one conversation.
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total - 1);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tether).await;

        // Check label1
        {
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 0);
        }
        // Check label2 - should be missing two messages.
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
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
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;

    // Deleting a conversation must
    // * Update conversation counters
    // * Update message counters

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::mark_deleted(local_label_id1, vec![local_conv_id], tx)
                .await
                .expect("failed to mark as deleted");
            Conversation::mark_deleted(local_label_id2, vec![local_conv_id], tx)
                .await
                .expect("failed to mark as deleted");

            Conversation::mark_undeleted(local_label_id1, vec![local_conv_id], tx)
                .await
                .expect("Failed to mark as undeleted");
            Conversation::mark_undeleted(local_label_id2, vec![local_conv_id], tx)
                .await
                .expect("Failed to mark as undeleted");
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
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
        let conv_counts = conv_counts_as_map(&tether).await;
        // Check conversation label1 values, should match original state.
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tether).await;

        // Check label1 - should match original state.
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_mark_read_no_message_metadata() {
    // Mark conversation as read without message metadata.
    let mut state = new_test_unread_db_state();
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut tether, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Remove all messages
            tx.execute("DELETE FROM messages", vec![]).await.unwrap();
            Conversation::mark_read_async(std::iter::once(local_conv_id), tx)
                .await
                .unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // No more unread messages
    assert_eq!(db_conversation.num_unread, 0);

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
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
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::mark_read_async(std::iter::once(local_conv_id), tx)
                .await
                .unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // No more unread messages
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 4);

    // Check conversation counts
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        // Check conversation label1 values, conversation should have been removed.
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check conversation label2 values - should be unchanged.
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts
    {
        let message_counts = msg_counts_as_map(&tether).await;

        // Check label1
        {
            let (unread, _) = message_counts_for_conversation(
                &state.messages,
                state.conversations[0].remote_id.as_ref().unwrap(),
                &MY_LABEL_ID1,
            );
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, start_label_counts.unread - unread);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
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
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut tether, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
    tether
        .sync_tx(move |tx: &Transaction<'_>| {
            // delete all messages.
            tx.execute("DELETE FROM messages", ()).unwrap();
            Conversation::mark_read([local_conv_id], tx).unwrap();
            Conversation::mark_unread(local_label_id1, [local_conv_id], tx).unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);

    // Check conversation counts match original values.
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        {
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID1.clone())
                .unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        {
            // Label2 should have no unread messages since the message in conv 1 is not the latest.
            let start_label_counts = state_map
                .conversation_counts
                .get(&MY_LABEL_ID2.clone())
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
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let state = new_test_unread_db_state();
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();

    tether
        .sync_tx(move |tx| {
            // First mark all msgs as unread
            Conversation::mark_read([local_conv_id], tx).unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    assert_eq!(db_conversation.num_messages, 4);
    assert_eq!(db_conversation.num_unread, 0);

    tether
        .sync_tx(move |tx| {
            // Mark last one as unread
            Conversation::mark_unread(local_label_id1, [local_conv_id], tx)?;
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    let messages = Message::find(
        "WHERE local_conversation_id=?
                AND unread=1",
        params![local_conv_id],
        &tether,
    )
    .await
    .unwrap();
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    // newest message has time at 400.
    assert_eq!(message.time, 400.into());
    assert!(message.unread);
    assert_eq!(message.label_ids[0], *MY_LABEL_ID1);

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);

    {
        let conv_counts = conv_counts_as_map(&tether).await;
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        {
            let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
            let label_counts = conv_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tether).await;

        // Check label1
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
            let label_counts = message_counts.get(&local_label_id1).unwrap();
            assert_eq!(label_counts.unread, 1);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
        // Check label2 - should be unchanged.
        {
            let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
            let label_counts = message_counts.get(&local_label_id2).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, start_label_counts.total);
        }
    }
}

#[tokio::test]
async fn test_conversation_marks_only_the_last_message_with_the_same_label_as_unread() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_unread_db_state_unread_label_in_folder();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    assert_eq!(db_conversation.num_messages, 2);
    assert_eq!(db_conversation.num_unread, 0);

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Mark last one as unread
            Conversation::mark_unread_async(local_label_id1, [local_conv_id], tx)
                .await
                .unwrap();
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    let messages = Message::find(
        "WHERE local_conversation_id=?",
        params![local_conv_id],
        &tether,
    )
    .await
    .unwrap();
    let read_message = &messages[0];
    let unread_message = &messages[1];
    assert_eq!(read_message.time, 100.into());
    assert_eq!(unread_message.time, 200.into());

    assert_eq!(read_message.label_ids[0], *MY_LABEL_ID1);
    assert_eq!(unread_message.label_ids[0], *MY_LABEL_ID2);

    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
}

#[tokio::test]
async fn mark_conversation_unread_does_nothing_if_already_unread() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_unread_db_state();
    for msg in &mut state.messages {
        msg.unread = false;
    }
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    assert_eq!(db_conversation.num_unread, 0);

    let modified = tether
        .tx::<_, _, StashError>(async |tx| {
            // Mark last one as unread
            Conversation::mark_unread_async(local_label_id1, [local_conv_id], tx).await
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    let messages = Message::find(
        "WHERE local_conversation_id=?",
        params![local_conv_id],
        &tether,
    )
    .await
    .unwrap();
    let unread_message = &messages[3];
    assert_eq!(modified.len(), 1);
    assert_eq!(unread_message.time, 400.into());
    assert_eq!(unread_message.id(), modified[0]);
    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(
        db_conversation
            .labels
            .iter()
            .find(|l| l.local_label_id.unwrap() == local_label_id1)
            .unwrap()
            .context_num_unread,
        1
    );

    // mark unread again, should be noop.
    let modified = tether
        .tx::<_, _, StashError>(async |tx| {
            // Mark last one as unread
            Conversation::mark_unread_async(local_label_id1, [local_conv_id], tx).await
        })
        .await
        .unwrap();

    let db_conversation = Conversation::load(local_conv_id, &tether)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    let messages = Message::find(
        "WHERE local_conversation_id=?",
        params![local_conv_id],
        &tether,
    )
    .await
    .unwrap();
    assert!(modified.is_empty());
    let unread_message = &messages[3];
    assert_eq!(unread_message.time, 400.into());
    // There should be 1 unread message.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(
        db_conversation
            .labels
            .iter()
            .find(|l| l.local_label_id.unwrap() == local_label_id1)
            .unwrap()
            .context_num_unread,
        1
    );
}

#[tokio::test]
async fn test_conversation_label_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
                .await
                .expect("failed to label");
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tether)
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
            .fold(UnixTimestamp::new(0), |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tether).await;
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
    let mut conn = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Ok(())
    })
    .await
    .unwrap();

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &conn)
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
            .fold(UnixTimestamp::new(0), |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&conn).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&conn).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 3);
    }
}

#[tokio::test]
async fn test_conversation_label_partially() {
    // Label conversation with a label where one of the messages already has been labeled
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let mut state = state.clone();
    state.messages[1].label_ids.push(MY_LABEL_ID1.clone());
    state.conversations[0].labels.push(
        ApiConversationLabel {
            id: MY_LABEL_ID1.clone(),
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
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
                .await
                .expect("failed to label");
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tether)
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
            .fold(UnixTimestamp::new(0), |x, m| x.max(m.expiration_time))
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts, only one message should be unread
    {
        let message_counts = msg_counts_as_map(&tether).await;
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
    let mut conn = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut conn, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Ok(())
    })
    .await
    .unwrap();

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &conn)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 0);
    assert_eq!(db_conversation.num_attachments, 0);
    assert_eq!(db_conversation.size, 0);
    assert_eq!(db_conversation.time, 0.into());
    assert_eq!(db_conversation.expiration_time, 0.into());
    assert_eq!(db_conversation.snoozed_until, None);

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&conn).await;
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
    let mut conn = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut conn, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Ok(())
    })
    .await
    .unwrap();

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &conn)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 0);
    assert_eq!(db_conversation.num_attachments, 0);
    assert_eq!(db_conversation.size, 0);
    assert_eq!(db_conversation.time, 0.into());
    assert_eq!(db_conversation.expiration_time, 0.into());
    assert_eq!(db_conversation.snoozed_until, None);

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&conn).await;
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
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state_label_with_existing_labels();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut tether, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
                .await
                .expect("failed to label");
            Ok(())
        })
        .await
        .unwrap();

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &tether)
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
        conv_label.context_expiration_time.into()
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tether).await;
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
    let mut conn = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Conversation::remove_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to unlabel");
        Ok(())
    })
    .await
    .unwrap();

    assert!(
        ContextualConversation::load(local_conv_id, local_label_id1, &conn)
            .await
            .expect("failed to get conversation")
            .is_none()
    );

    // Check conversation counts should be 0
    {
        let conv_counts = conv_counts_as_map(&conn).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 0);
    }

    // Check message counts should be 0
    {
        let message_counts = msg_counts_as_map(&conn).await;
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
    let mut conn = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut conn, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Conversation::remove_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Ok(())
    })
    .await
    .unwrap();

    assert!(
        ContextualConversation::load(local_conv_id, local_label_id1, &conn)
            .await
            .expect("failed to get conversation")
            .is_none()
    );

    // Check conversation counts should be 0
    {
        let conv_counts = conv_counts_as_map(&conn).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 0);
    }
}

#[tokio::test]
async fn test_conversation_expiration() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut tether, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();

    // Delete all expired, no matches
    let res = Conversation::delete_expired(&mut tether).await.unwrap();
    assert_eq!(res, 0);

    let cv = Conversation::load(local_conv_id, &tether)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cv.expiration_time, 0.into());
    assert_eq!(cv.deleted, false);

    // Load a conversation
    Conversation::set_expiration_time_in(local_conv_id, -1000, &mut tether)
        .await
        .unwrap();

    // Delete all expired
    let res = Conversation::delete_expired(&mut tether).await.unwrap();

    assert_eq!(res, 1);

    // Check if it's deleted
    let cv = Conversation::load(local_conv_id, &tether)
        .await
        .unwrap()
        .unwrap();

    // Check that all messages are deleted too
    let messages = cv.load_messages(&tether).await.unwrap();
    for message in messages {
        assert_eq!(message.deleted, true);
    }

    assert_eq!(cv.deleted, true);
}

#[tokio::test]
async fn test_conversation_watcher() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut tether, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
                .await
                .expect("failed to label");
            Ok(())
        })
        .await
        .unwrap();

    let handle = ContextualConversation::watch(&stash).await.unwrap();
    let watch_result = &handle.receiver;

    tokio::spawn(async move {
        //bypass model to only execute exactly 2 queries.
        tether
            .tx::<_, _, StashError>(async |tx| {
                tx.execute("UPDATE conversation_labels SET context_num_unread=? WHERE local_label_id=? AND local_conversation_id=?",
                           params![30, local_label_id1, local_conv_id],
                ).await.unwrap();
                tx.execute(
                    "UPDATE conversations SET num_unread=? WHERE local_id=?",
                    params![10, local_conv_id],
                )
                    .await
                    .unwrap();
                Ok(())
            }).await.unwrap();
    });

    watch_result.recv_async().await.unwrap();
}

#[tokio::test]
async fn test_contextual_conversation_messages() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut tether, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();

    let handle = ContextualConversation::watch(&stash).await.unwrap();
    let watch_result = &handle.receiver;

    tether
        .tx::<_, _, StashError>(async |tx| {
            Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
                .await
                .expect("failed to label");
            Ok(())
        })
        .await
        .unwrap();

    watch_result.recv_async().await.unwrap();
}

static STARRED: LazyLock<Label> =
    LazyLock::new(|| label!(label_type: LabelType::System, remote_id: Some(LabelId::starred())));
static FOLDER: LazyLock<Label> = LazyLock::new(
    || label!(label_type: LabelType::Folder, remote_id: Some("folder_label".into()), name: "MyFavouritesFolder".to_owned(), color: LabelColor::black()),
);
static INBOX: LazyLock<Label> = LazyLock::new(
    || label!(label_type: LabelType::System, remote_id: Some(LabelId::inbox()), name: "Inbox".to_owned(), color: LabelColor::black()),
);
static LABEL: LazyLock<Label> = LazyLock::new(
    || label!(label_type: LabelType::Label, remote_id: Some("label".into()), name: "Label".to_owned(), color: LabelColor::black()),
);

#[test_case(vec![], None; "TEST1 - no label")]
#[test_case(
    vec![LABEL.clone(), FOLDER.clone(), STARRED.clone()],
    Some((false, "MyFavouritesFolder")); "TEST2 - mixed labels - custom")]
#[test_case(
    vec![LABEL.clone(), FOLDER.clone(), STARRED.clone(), INBOX.clone()],
    Some((true, "inbox")); "TEST3 - mixed labels - system")]
#[test_case(
    vec![LABEL.clone(), STARRED.clone()],
    None; "TEST4 - no folder")]
#[tokio::test]
async fn conversation_exclusive_location_on_save(
    labels: Vec<Label>,
    expected: Option<(bool, &str)>,
) {
    // Setup:
    //   * create a conversation with some labels
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    let mut conversation = Conversation {
        ..Conversation::test_default()
    };
    let mut conversation_labels = Vec::with_capacity(labels.len());
    tether
        .tx::<_, _, StashError>(async |tx| {
            conversation.save(tx).await.unwrap();
            for mut label in labels {
                label.save(tx).await.unwrap();
                conversation_labels.push(ConversationLabel {
                    remote_label_id: label.remote_id,
                    ..ConversationLabel::test_default()
                });
            }
            conversation.labels = conversation_labels;

            // Action
            conversation.save(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Validation
    if let Some((is_system, expected)) = expected {
        match conversation.locations.first().unwrap() {
            ExclusiveLocation::System { name, .. } => {
                assert!(is_system);
                match name {
                    SystemLabel::Inbox => assert_eq!("inbox", expected),
                    _ => panic!("expected SystemLabel: {name}"),
                }
            }
            ExclusiveLocation::Custom { name, .. } => {
                assert!(!is_system);
                assert_eq!(name, expected)
            }
        }
    } else {
        assert!(conversation.locations.is_empty());
    }
}

#[tokio::test]
async fn test_conversation_move_to() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection().await.unwrap();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) =
        prepare_and_patch_db_state_and_skip(&mut conn, state.clone(), true).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Ok(())
    })
    .await
    .unwrap();

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &conn)
        .await
        .expect("failed to get conversation")
        .expect("should have value");

    // Because we have no message metadata, all these values should be empty
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 0);
    assert_eq!(db_conversation.num_attachments, 0);
    assert_eq!(db_conversation.size, 0);
    assert_eq!(db_conversation.time, 0.into());
    assert_eq!(db_conversation.expiration_time, 0.into());
    assert_eq!(db_conversation.snoozed_until, None);

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&conn).await;
        {
            let label_counts = conv_counts.get(&local_label_id1).unwrap();
            // unread is 0 due to lack of messages.
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 1);
        }
    }
}

#[tokio::test]
async fn conversation_save_updates_local_ids_for_attachment_metadata() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let regular_attachment_id = AttachmentId::from("regular-att");
    let api_conversation = ApiConversation {
        id: ConversationId::from("My-Conv"),
        attachment_info: Default::default(),
        attachments_metadata: vec![ApiAttachmentMetadata {
            id: regular_attachment_id.clone(),
            disposition: proton_mail_api::services::proton::prelude::Disposition::Attachment,
            mime_type: "application/pdf".to_string(),
            name: "file.pdf".to_string(),
            size: 1024,
        }],
        display_snoozed_reminder: false,
        expiration_time: 0,
        labels: vec![],
        num_attachments: 0,
        num_messages: 0,
        num_unread: 0,
        order: 0,
        recipients: vec![],
        senders: vec![],
        size: 0,
        subject: "".to_string(),
        context_time: None,
    };
    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut conv = Conversation::from(api_conversation);
            conv.save(tx).await?;
            assert!(
                conv.attachments_metadata
                    .iter()
                    .all(|a| a.local_id.is_some())
            );
            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn conversation_snooze_without_message_metadata() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let initial_time = UnixTimestamp::new(4096);
    let snooze_time = UnixTimestamp::now().saturating_add(8096);
    let api_conversation = ApiConversation {
        id: ConversationId::from("My-Conv"),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snoozed_reminder: false,
        expiration_time: 0,
        labels: vec![
            ApiConversationLabel {
                id: LabelId::inbox(),
                context_expiration_time: 0,
                context_num_attachments: 0,
                context_num_messages: 2,
                context_num_unread: 0,
                context_size: 1025,
                context_snooze_time: initial_time.as_u64(),
                context_time: initial_time.as_u64(),
            },
            ApiConversationLabel {
                id: LabelId::all_mail(),
                context_expiration_time: 0,
                context_num_attachments: 0,
                context_num_messages: 2,
                context_num_unread: 0,
                context_size: 1025,
                context_snooze_time: initial_time.as_u64(),
                context_time: initial_time.as_u64(),
            },
        ],
        num_attachments: 0,
        num_messages: 0,
        num_unread: 0,
        order: 0,
        recipients: vec![],
        senders: vec![],
        size: 0,
        subject: "".to_string(),
        context_time: None,
    };
    let inbox_label_id = Label::remote_id_counterpart(LabelId::inbox(), &tether)
        .await
        .unwrap()
        .unwrap();
    let snooze_label_id = Label::remote_id_counterpart(LabelId::snoozed(), &tether)
        .await
        .unwrap()
        .unwrap();
    let mut local_conv: Conversation = tether
        .tx::<_, _, MailContextError>(async |tx| {
            let mut conv = Conversation::from(api_conversation);
            conv.save(tx).await?;
            Conversation::snooze(inbox_label_id, &[conv.id()], snooze_time, tx).await?;
            Ok(conv)
        })
        .await
        .unwrap();

    local_conv.reload(&tether).await.unwrap();

    assert_eq!(local_conv.snoozed_until, Some(snooze_time));
    let snooze_label = local_conv
        .labels
        .iter()
        .find(|v| v.local_label_id.unwrap() == snooze_label_id)
        .unwrap();
    assert_eq!(snooze_label.context_snooze_time, snooze_time);
    assert_eq!(snooze_label.context_time, initial_time);
    assert!(
        !local_conv
            .labels
            .iter()
            .any(|v| v.local_label_id.unwrap() == inbox_label_id)
    );

    // undo snooze
    tether
        .tx::<_, _, MailContextError>(async |tx| {
            Conversation::unsnooze(snooze_label_id, &[local_conv.id()], tx).await?;
            Ok(())
        })
        .await
        .unwrap();

    local_conv.reload(&tether).await.unwrap();

    assert!(local_conv.snoozed_until.is_none());
    let inbox_label = local_conv
        .labels
        .iter()
        .find(|v| v.local_label_id.unwrap() == inbox_label_id)
        .unwrap();
    assert_eq!(inbox_label.context_snooze_time, initial_time);
    assert_eq!(inbox_label.context_time, initial_time);
    assert!(
        !local_conv
            .labels
            .iter()
            .any(|v| v.local_label_id.unwrap() == snooze_label_id)
    );
}

#[tokio::test]
async fn conversation_snooze_only_snoozes_received_messages_in_inbox() {
    // 1 conversation with the following messages:
    // * Inbox + Custom Label - received
    // * Sent - sent/replied
    // * Custom folder - received
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection().await.unwrap();
    let mut state = new_conversation_snooze_db_state();
    let snooze_time = UnixTimestamp::now().saturating_add(8096);
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let inbox_label_id = Label::remote_id_counterpart(LabelId::inbox(), &conn)
        .await
        .unwrap()
        .unwrap();
    let snooze_label_id = Label::remote_id_counterpart(LabelId::snoozed(), &conn)
        .await
        .unwrap()
        .unwrap();

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();

    let local_msg_id_1 = *state_map
        .messages
        .get(state.messages[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id_2 = *state_map
        .messages
        .get(state.messages[1].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id_3 = *state_map
        .messages
        .get(state.messages[2].remote_id.as_ref().unwrap())
        .unwrap();

    conn.tx(async |tx| {
        Conversation::snooze(inbox_label_id, &[local_conv_id], snooze_time, tx).await
    })
    .await
    .unwrap();

    let mut conv = Conversation::find_by_id(local_conv_id, &conn)
        .await
        .unwrap()
        .unwrap();
    let mut msg_1 = Message::find_by_id(local_msg_id_1, &conn)
        .await
        .unwrap()
        .unwrap();
    let mut msg_2 = Message::find_by_id(local_msg_id_2, &conn)
        .await
        .unwrap()
        .unwrap();
    let mut msg_3 = Message::find_by_id(local_msg_id_3, &conn)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(conv.snoozed_until, Some(snooze_time));
    let snooze_label = conv
        .labels
        .iter()
        .find(|v| v.local_label_id.unwrap() == snooze_label_id)
        .unwrap();
    assert_eq!(snooze_label.context_snooze_time, snooze_time);
    assert!(
        !conv
            .labels
            .iter()
            .any(|v| v.local_label_id.unwrap() == inbox_label_id)
    );

    // Message 1 should be snoozed
    assert_eq!(msg_1.snooze_time, snooze_time);
    assert_eq!(msg_1.snoozed_until(), Some(snooze_time));
    assert!(msg_1.label_ids.contains(&LabelId::snoozed()));
    assert!(msg_1.label_ids.contains(&MY_LABEL_ID1));
    assert!(!msg_1.label_ids.contains(&LabelId::inbox()));

    // Message 2 and 3 remain unaffected.
    assert_eq!(msg_2.snooze_time, 0.into());
    assert!(msg_2.snoozed_until().is_none());
    assert!(!msg_2.label_ids.contains(&LabelId::snoozed()));
    assert!(!msg_2.label_ids.contains(&LabelId::inbox()));
    assert!(msg_2.label_ids.contains(&LabelId::sent()));

    assert_eq!(msg_3.snooze_time, 0.into());
    assert!(msg_3.snoozed_until().is_none());
    assert!(!msg_3.label_ids.contains(&LabelId::snoozed()));
    assert!(!msg_3.label_ids.contains(&LabelId::inbox()));
    assert!(msg_3.label_ids.contains(&MY_LABEL_ID2));

    // unsooze the conversation

    conn.tx(async |tx| Conversation::unsnooze(snooze_label_id, &[local_conv_id], tx).await)
        .await
        .unwrap();
    conv.reload(&conn).await.unwrap();
    msg_1.reload(&conn).await.unwrap();
    msg_2.reload(&conn).await.unwrap();
    msg_3.reload(&conn).await.unwrap();

    assert!(conv.snoozed_until.is_none());
    let inbox_label = conv
        .labels
        .iter()
        .find(|v| v.local_label_id.unwrap() == inbox_label_id)
        .unwrap();
    assert_eq!(inbox_label.context_snooze_time, state.messages[0].time);
    assert!(
        !conv
            .labels
            .iter()
            .any(|v| v.local_label_id.unwrap() == snooze_label_id)
    );

    // Message 1 should be returned to inbox
    assert_eq!(msg_1.snooze_time, state.messages[0].time);
    assert!(msg_1.snoozed_until().is_none());
    assert!(!msg_1.label_ids.contains(&LabelId::snoozed()));
    assert!(msg_1.label_ids.contains(&MY_LABEL_ID1));
    assert!(msg_1.label_ids.contains(&LabelId::inbox()));

    // Message 2 and 3 remain unaffected.
    assert_eq!(msg_2.snooze_time, 0.into());
    assert!(msg_2.snoozed_until().is_none());
    assert!(!msg_2.label_ids.contains(&LabelId::snoozed()));
    assert!(!msg_2.label_ids.contains(&LabelId::inbox()));
    assert!(msg_2.label_ids.contains(&LabelId::sent()));

    assert_eq!(msg_3.snooze_time, 0.into());
    assert!(msg_3.snoozed_until().is_none());
    assert!(!msg_3.label_ids.contains(&LabelId::snoozed()));
    assert!(!msg_3.label_ids.contains(&LabelId::inbox()));
    assert!(msg_3.label_ids.contains(&MY_LABEL_ID2));
}

#[tokio::test]
async fn conversation_expiration() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let expiration_time = UnixTimestamp::now().saturating_sub(20);
    let api_conversation = ApiConversation {
        id: ConversationId::from("My-Conv"),
        attachment_info: Default::default(),
        attachments_metadata: vec![],
        display_snoozed_reminder: false,
        expiration_time: expiration_time.as_u64(),
        labels: vec![ApiConversationLabel {
            id: LabelId::inbox(),
            context_expiration_time: 0,
            context_num_attachments: 0,
            context_num_messages: 2,
            context_num_unread: 0,
            context_size: 1025,
            context_snooze_time: 0,
            context_time: 0,
        }],
        num_attachments: 0,
        num_messages: 0,
        num_unread: 0,
        order: 0,
        recipients: vec![],
        senders: vec![],
        size: 0,
        subject: "".to_string(),
        context_time: None,
    };
    let mut local_conv: Conversation = tether
        .tx::<_, _, MailContextError>(async |tx| {
            let mut conv = Conversation::from(api_conversation);
            conv.save(tx).await?;
            Ok(conv)
        })
        .await
        .unwrap();

    Conversation::delete_expired(&mut tether).await.unwrap();

    local_conv.reload(&tether).await.unwrap();
    assert!(local_conv.deleted);
}

#[tokio::test]
async fn test_conversation_label_set_lowest_expiration_time_in_label_context() {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection().await.unwrap();
    let mut state = new_test_label_expiration_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        Conversation::apply_label_async(local_label_id1, vec![local_conv_id], tx)
            .await
            .expect("failed to label");
        Ok(())
    })
    .await
    .unwrap();

    let conv = Conversation::load(local_conv_id, &conn)
        .await
        .expect("failed to get conversation")
        .unwrap();

    let conv_label = conv
        .labels
        .iter()
        .find(|&l| l.local_label_id.unwrap() == local_label_id1)
        .unwrap();
    let lowest_expiration_time = state
        .messages
        .iter()
        .fold(UnixTimestamp::new(u64::MAX), |v1, v2| {
            v1.min(v2.expiration_time)
        });
    assert_eq!(
        conv_label.context_expiration_time,
        lowest_expiration_time.into()
    );
}

#[tokio::test]
#[test_case::test_case(
    create_or_get_local_default_conv(),
    create_or_get_local_default_with_updated_label(),
    true;
    "Updated label syncs"
)]
#[test_case::test_case(
    create_or_get_local_default_conv(),
    create_or_get_local_default_with_custom_label_only(),
    true;
    "Remove label syncs"
)]
#[test_case::test_case(
    create_or_get_local_default_with_custom_label_only(),
    create_or_get_local_default_conv(),
    true;
    "Added label syncs"
)]
#[test_case::test_case(
    create_or_get_local_default_conv(),
    create_or_get_local_default_with_both_labels(),
    false;
    "Unrelated label update does not sync"
)]
async fn create_or_get_local(
    mut existing_conversation: Conversation,
    mut new_conversation: Conversation,
    expect_replace: bool,
) {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection().await.unwrap();
    let mut state = TestDBState {
        addresses: vec![],
        labels: vec![
            test_label1(),
            label!(
               remote_id: Some(LabelId::inbox()),
               name: "Inbox".to_owned(),
               path: Some("Inbox".to_owned()),
               color: LabelColor::black(),
               label_type: LabelType::System,
               display_order: 0
            ),
        ],
        conversations: vec![],
        messages: vec![],
    };
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let _ = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let mut tether = stash.connection().await.unwrap();

    tether
        .tx(async |tx| {
            let mut change_set = RebaseChangeSet::default();
            existing_conversation.save(tx).await?;
            existing_conversation.reload(tx).await?;
            new_conversation
                .create_or_get_local(&LabelId::inbox(), &mut change_set, tx)
                .await?;
            new_conversation.reload(tx).await
        })
        .await
        .unwrap();

    let db_conv = Conversation::find_by_id(existing_conversation.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    if expect_replace {
        assert_eq!(db_conv, new_conversation);
        assert_ne!(new_conversation, existing_conversation);
    } else {
        assert_eq!(db_conv, existing_conversation);
        assert_eq!(new_conversation, existing_conversation);
    }
}

fn create_or_get_local_default_conv() -> Conversation {
    Conversation {
        remote_id: Some(ConversationId::from("MY_CONV")),
        labels: vec![ConversationLabel {
            remote_label_id: Some(LabelId::inbox()),
            context_num_messages: 1,
            ..ConversationLabel::test_default()
        }],
        num_messages: 1,
        num_unread: 0,
        is_known: true,
        ..Conversation::test_default()
    }
}

fn create_or_get_local_default_with_updated_label() -> Conversation {
    Conversation {
        remote_id: Some(ConversationId::from("MY_CONV")),
        labels: vec![ConversationLabel {
            remote_label_id: Some(LabelId::inbox()),
            context_num_messages: 2,
            context_num_unread: 1,
            ..ConversationLabel::test_default()
        }],
        num_messages: 1,
        num_unread: 0,
        is_known: true,
        ..Conversation::test_default()
    }
}
fn create_or_get_local_default_with_custom_label_only() -> Conversation {
    Conversation {
        remote_id: Some(ConversationId::from("MY_CONV")),
        labels: vec![ConversationLabel {
            remote_label_id: Some(test_label1().remote_id.unwrap()),
            context_num_messages: 0,
            context_num_unread: 1,
            ..ConversationLabel::test_default()
        }],
        num_messages: 1,
        num_unread: 0,
        is_known: true,
        ..Conversation::test_default()
    }
}
fn create_or_get_local_default_with_both_labels() -> Conversation {
    Conversation {
        remote_id: Some(ConversationId::from("MY_CONV")),
        labels: vec![
            ConversationLabel {
                remote_label_id: Some(LabelId::inbox()),
                context_num_messages: 1,
                ..ConversationLabel::test_default()
            },
            ConversationLabel {
                remote_label_id: Some(test_label1().remote_id.unwrap()),
                context_num_messages: 0,
                context_num_unread: 1,
                ..ConversationLabel::test_default()
            },
        ],
        num_messages: 1,
        num_unread: 0,
        is_known: true,
        ..Conversation::test_default()
    }
}
