#![allow(non_snake_case)]

use std::sync::LazyLock;

use super::*;
use crate::actions::{
    LabelAsAction, MessageAction, MessageAvailableActions, MovableSystemFolderAction, MoveAction,
    MoveItemAction,
};
use crate::datatypes::{
    ContextualConversation, ExclusiveLocation, MessageFlags, MessageLabelsCount,
    MovableSystemFolder, SystemLabelId, attachment,
};
use crate::label;
use crate::models::{Conversation, MailSettings, Message, MessageBodyMetadata};
use crate::test_utils::db::new_test_connection_file;
use crate::test_utils::db_states::{
    new_test_delete_db_state, new_test_label_db_state, new_test_unread_db_state,
};
use crate::test_utils::search::{
    MY_ADDRESS_ID, MY_CONVERSATION_ID, MY_LABEL_ID1, MY_LABEL_ID2, create_labels,
    test_conversation, test_starred_label,
};
use crate::test_utils::utils::{
    conv_counts_as_map, find_conversation_label, msg_counts_as_map, prepare_and_patch_db_state,
    prepare_db_state_core,
};
use crate::test_utils::utils::{create_address, test_address};
use crate::{conv_id, conversation, message, msg_id};
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt as _};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LabelColor, LabelType};
use proton_core_common::models::Label;
use proton_crypto_inbox::attachment::KeyPackets;
use proton_mail_api::services::proton::common::AttachmentId;
use proton_mail_api::services::proton::response_data::MessageMetadata as ApiMessageMetadata;
use proton_mail_api::services::proton::response_data::{
    AttachmentMetadata as ApiAttachmentMetadata, ConversationLabel as ApiConversationLabel,
    Disposition as ApiDisposition, Message as ApiMessage,
    MessageAttachment as ApiMessageAttachment,
    MessageAttachmentHeaders as ApiMessageAttachmentHeaders, MessageFlags as ApiMessageFlags,
    MessageSender as ApiMessageSender, MimeType as ApiMimeType,
};
use serde_json::json;
use stash::orm::Model;
use stash::params;
use stash::stash::{Stash, Tether};
use test_case::test_case;
use velcro::hash_map;

static STARRED: LazyLock<Label> =
    LazyLock::new(|| label!(label_type: LabelType::System, remote_id: Some(LabelId::starred())));
static FOLDER: LazyLock<Label> = LazyLock::new(
    || label!(label_type: LabelType::Folder, remote_id: Some("folder_label".into()), name: "MyFavouritesFolder".to_owned(), color: LabelColor::black()),
);
static INBOX: LazyLock<Label> = LazyLock::new(
    || label!(label_type: LabelType::System, remote_id: Some(LabelId::inbox()), name: "Inbox".to_owned(), color: LabelColor::black()),
);
static SPAM: LazyLock<Label> = LazyLock::new(
    || label!(label_type: LabelType::System, remote_id: Some(LabelId::spam()), name: "Spam".to_owned(), color: LabelColor::black()),
);
static LABEL: LazyLock<Label> = LazyLock::new(
    || label!(label_type: LabelType::Label, remote_id: Some("label".into()), name: "Label".to_owned(), color: LabelColor::black()),
);

mod available_actions {
    use std::sync::LazyLock;

    use crate::actions::GeneralActions;
    use crate::datatypes::theme::MailTheme;

    use super::*;
    use crate::test_utils::db::new_test_connection;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    struct TestCase {
        view: Label,
        message: MessageWithLabels,
        theme: ThemeOpts,
        expected: Result<MessageAvailableActions, AppError>,
    }

    #[derive(Clone)]
    struct MessageWithLabels {
        message: Message,
        labels: Vec<Label>,
    }

    static TEST1: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: INBOX.clone(),
        message: MessageWithLabels {
            message: message!(unread: true, remote_id: msg_id!("test1")),
            labels: vec![STARRED.clone(), FOLDER.clone()],
        },
        theme: ThemeOpts {
            current_theme: MailTheme::LightMode,
            theme_override: None,
            supports_dark_mode_via_media_query: false,
        },
        expected: Ok(MessageAvailableActions::builder()
            .move_actions(vec![
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Trash,
                }
                .into(),
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                }
                .into(),
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Spam,
                }
                .into(),
                MoveItemAction::MoveTo,
            ])
            .message_actions(vec![
                MessageAction::MarkRead,
                MessageAction::Unstar,
                MessageAction::LabelAs,
            ])
            .general_actions(vec![
                GeneralActions::Print,
                GeneralActions::ReportPhishing,
                GeneralActions::SaveAsPdf,
                GeneralActions::ViewHeaders,
                GeneralActions::ViewHtml,
            ])
            .build()),
    });

    static TEST2: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: FOLDER.clone(),
        message: MessageWithLabels {
            message: message!(unread: false, remote_id: Some("test2".into())),
            labels: vec![FOLDER.clone()],
        },
        theme: ThemeOpts {
            current_theme: MailTheme::LightMode,
            theme_override: None,
            supports_dark_mode_via_media_query: false,
        },
        expected: Ok(MessageAvailableActions::builder()
            .move_actions(vec![
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Trash,
                }
                .into(),
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                }
                .into(),
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Spam,
                }
                .into(),
                MoveItemAction::MoveTo,
            ])
            .message_actions(vec![
                MessageAction::MarkUnread,
                MessageAction::Star,
                MessageAction::LabelAs,
            ])
            .general_actions(vec![
                GeneralActions::Print,
                GeneralActions::ReportPhishing,
                GeneralActions::SaveAsPdf,
                GeneralActions::ViewHeaders,
                GeneralActions::ViewHtml,
            ])
            .build()),
    });

    static TEST3: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: SPAM.clone(),
        message: MessageWithLabels {
            message: message!(remote_id: Some("test3".into())),
            labels: vec![],
        },
        theme: ThemeOpts {
            current_theme: MailTheme::LightMode,
            theme_override: None,
            supports_dark_mode_via_media_query: false,
        },
        expected: Ok(MessageAvailableActions::builder()
            .move_actions(vec![
                MoveItemAction::PermanentDelete,
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                }
                .into(),
                MoveItemAction::NotSpam(MovableSystemFolderAction {
                    local_id: 1.into(),
                    name: MovableSystemFolder::Inbox,
                }),
                MoveItemAction::MoveTo,
            ])
            .message_actions(vec![
                MessageAction::MarkUnread,
                MessageAction::Star,
                MessageAction::LabelAs,
            ])
            .general_actions(vec![
                GeneralActions::Print,
                GeneralActions::ReportPhishing,
                GeneralActions::SaveAsPdf,
                GeneralActions::ViewHeaders,
                GeneralActions::ViewHtml,
            ])
            .build()),
    });

    static TEST4: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: INBOX.clone(),
        message: MessageWithLabels {
            message: message!(unread: true, remote_id: msg_id!("test4")),
            labels: vec![STARRED.clone(), FOLDER.clone()],
        },
        theme: ThemeOpts {
            current_theme: MailTheme::DarkMode,
            theme_override: None,
            supports_dark_mode_via_media_query: false,
        },
        expected: Ok(MessageAvailableActions::builder()
            .move_actions(vec![
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Trash,
                }
                .into(),
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                }
                .into(),
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Spam,
                }
                .into(),
                MoveItemAction::MoveTo,
            ])
            .message_actions(vec![
                MessageAction::MarkRead,
                MessageAction::Unstar,
                MessageAction::LabelAs,
            ])
            .general_actions(vec![
                GeneralActions::Print,
                GeneralActions::ReportPhishing,
                GeneralActions::SaveAsPdf,
                GeneralActions::ViewHeaders,
                GeneralActions::ViewHtml,
                GeneralActions::ViewMessageInLightMode,
            ])
            .build()),
    });

    static TEST5: LazyLock<TestCase> = LazyLock::new(|| TestCase {
        view: INBOX.clone(),
        message: MessageWithLabels {
            message: message!(unread: true, remote_id: msg_id!("test5")),
            labels: vec![STARRED.clone(), FOLDER.clone()],
        },
        theme: ThemeOpts {
            current_theme: MailTheme::DarkMode,
            theme_override: Some(MailTheme::LightMode),
            supports_dark_mode_via_media_query: false,
        },
        expected: Ok(MessageAvailableActions::builder()
            .move_actions(vec![
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Trash,
                }
                .into(),
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Archive,
                }
                .into(),
                MovableSystemFolderAction {
                    local_id: 0.into(),
                    name: MovableSystemFolder::Spam,
                }
                .into(),
                MoveItemAction::MoveTo,
            ])
            .message_actions(vec![
                MessageAction::MarkRead,
                MessageAction::Unstar,
                MessageAction::LabelAs,
            ])
            .general_actions(vec![
                GeneralActions::Print,
                GeneralActions::ReportPhishing,
                GeneralActions::SaveAsPdf,
                GeneralActions::ViewHeaders,
                GeneralActions::ViewHtml,
                GeneralActions::ViewMessageInDarkMode,
            ])
            .build()),
    });

    #[test_case(&TEST1; "TEST1: Unread, starred in custom folder viewed from Inbox")]
    #[test_case(&TEST2; "TEST2: Read, not starred and in custom folder viewed from Folder")]
    #[test_case(&TEST3; "TEST3: Default, viewed from Spam")]
    #[test_case(&TEST4; "TEST4: Dark mode, no override")]
    #[test_case(&TEST5; "TEST5: Dark mode, with override")]
    #[tokio::test]
    async fn test_available_actions(test_case: &TestCase) {
        let stash = new_test_connection().await;
        let mut conn = stash.connection();
        let mut message_id = None;
        let address = create_address(&mut conn).await;
        let mut conversation = conversation!(remote_id: Some("test_conversation".into()));

        conn.tx::<_, _, StashError>(async |tx| {
            conversation.save(tx).await.unwrap();
            let MessageWithLabels {
                mut message,
                labels,
            } = test_case.message.clone();
            message.local_address_id = address.id();
            message.remote_address_id = address.remote_id.clone().unwrap();
            message.local_conversation_id = conversation.local_id;
            message.remote_conversation_id = conversation.remote_id.clone();

            message.save(tx).await.expect("failed to create message");

            message_id = Some(message.id());

            for mut label in labels {
                label.save(tx).await.expect("failed to create label");
                let local_id = label.id();
                ConversationCounters::new(local_id)
                    .save(tx)
                    .await
                    .expect("Failed to create counters");
                MessageCounters::new(local_id)
                    .save(tx)
                    .await
                    .expect("Failed to create counters");

                let label_id = label.id();
                let ids = vec![message.id()];

                Message::apply_label(label_id, ids, tx).await.unwrap();
            }
            Ok(())
        })
        .await
        .unwrap();

        let view = Label::find_by_remote_id(test_case.view.remote_id.clone().unwrap(), &conn)
            .await
            .unwrap()
            .unwrap();

        let result = Message::available_actions(
            view,
            message_id.expect("Message id"),
            test_case.theme,
            &conn,
        )
        .await;

        match result {
            Ok(mut actual) => {
                actual.move_actions.iter_mut().for_each(|action| {
                    if let MoveItemAction::MoveToSystemFolder(action) = action {
                        action.local_id = 0.into(); // To be able to compare with expected
                    }
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
    use crate::test_utils::db::new_test_connection;
    use crate::{conv_id, conversation, label, lbl_id, message, msg_id};
    use test_case::test_case;

    struct MessageWithLabels {
        message: Message,
        labels: Vec<Label>,
    }

    #[test_case(vec![], vec![], Err(AppError::EmptyListOfMessages); "TEST1: empty")]
    #[test_case(
        vec![
            MessageWithLabels { message: message!(remote_id: msg_id!("message_1")), labels: vec![] },
            MessageWithLabels { message: message!(remote_id: msg_id!("message_2")), labels: vec![] },
        ],
        vec![
            label!(remote_id: lbl_id!("label1"), label_type: LabelType::Label, name: "label1".to_string(), color: LabelColor::purple()),
            label!(remote_id: lbl_id!("label2"), label_type: LabelType::Label, name: "label2".to_string()),
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
        ]); "TEST2: messages without labels")]
    #[test_case(
        vec![
            MessageWithLabels { message: message!(remote_id: msg_id!("message_1")), labels: vec![
                label!(remote_id: lbl_id!("label1"), label_type: LabelType::Label, name: "label1".to_string(), color: LabelColor::purple()),
                label!(remote_id: lbl_id!("label2"), label_type: LabelType::Label, name: "label2".to_string()),
            ] },
            MessageWithLabels { message: message!(remote_id: msg_id!("message_2")), labels: vec![
                label!(remote_id: lbl_id!("label1"), label_type: LabelType::Label, name: "label1".to_string(), color: LabelColor::purple()),
                label!(remote_id: lbl_id!("label2"), label_type: LabelType::Label, name: "label2".to_string()),
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
        ]); "TEST3: messages with all labels")]
    #[test_case(
        vec![
            MessageWithLabels { message: message!(remote_id: msg_id!("message_1")), labels: vec![
                label!(remote_id: lbl_id!("label1"), label_type: LabelType::Label, name: "label1".to_string(), color: LabelColor::purple()),
            ] },
            MessageWithLabels { message: message!(remote_id: msg_id!("message_2")), labels: vec![
                label!(remote_id: lbl_id!("label2"), label_type: LabelType::Label, name: "label2".to_string()),
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
        ]); "TEST4: each message with different label")]
    #[tokio::test]
    async fn test_label_as_actions(
        messages: Vec<MessageWithLabels>,
        labels: Vec<Label>,
        expected: Result<&[LabelAsAction], AppError>,
    ) {
        let stash = new_test_connection().await;
        let mut conn = stash.connection();
        let address = create_address(&mut conn).await;
        let mut conversation = conversation!(remote_id: conv_id!("conversation"));
        let mut message_ids = vec![];
        conn.tx::<_, _, StashError>(async |tx| {
            conversation.save(tx).await.unwrap();

            for mut label in labels {
                label.save(tx).await.expect("failed to create label");
                MessageCounters::new(label.id())
                    .save(tx)
                    .await
                    .expect("failed to create message counters");
            }

            for MessageWithLabels {
                mut message,
                labels: message_labels,
            } in messages
            {
                message.local_address_id = address.id();
                message.remote_address_id = address.remote_id.clone().unwrap();
                message.local_conversation_id = conversation.local_id;
                message.remote_conversation_id = conversation.remote_id.clone();

                message.save(tx).await.expect("failed to create message");

                message_ids.push(message.id());

                for mut label in message_labels {
                    label.save(tx).await.expect("failed to create label");
                    let label_id = label.id();
                    ConversationCounters::new(label_id)
                        .save(tx)
                        .await
                        .expect("failed to create conversation counters");
                    MessageCounters::new(label_id)
                        .save(tx)
                        .await
                        .expect("failed to create message counters");

                    let ids = vec![message.id()];

                    Message::apply_label(label_id, ids, tx).await.unwrap();
                }
            }
            Ok(())
        })
        .await
        .unwrap();

        let result = Message::available_label_as_actions(message_ids, &conn).await;

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
    use crate::test_utils::db::new_test_connection;
    use crate::{conv_id, conversation, label, lbl_id, message, msg_id};
    use futures::stream::{self, StreamExt};
    use pretty_assertions::assert_eq;
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

    struct MessageWithLabels {
        message: Message,
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
        || label!(label_type: LabelType::Folder, remote_id: lbl_id!("0123"), name: "My custom folder".to_owned(), color: LabelColor::purple()),
    );

    #[test_case(&INBOX, vec![], vec![], Err(AppError::EmptyListOfMessages); "TEST1: empty")]
    #[test_case(
        &INBOX,
        vec![
            MessageWithLabels { message: message!(remote_id: msg_id!("message_1")), labels: vec![] },
            MessageWithLabels { message: message!(remote_id: msg_id!("message_2")), labels: vec![] },
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
        ]); "TEST2: messages without labels")]
    #[test_case(
        &INBOX,
        vec![
            MessageWithLabels { message: message!(remote_id: msg_id!("message_1")), labels: vec![INBOX.clone()] },
            MessageWithLabels { message: message!(remote_id: msg_id!("message_2")), labels: vec![label!(remote_id: lbl_id!("label2"), label_type: LabelType::Folder, name: "label2".to_string())] },
        ],
        vec![
            label!(remote_id: lbl_id!("label1"), label_type: LabelType::Folder, name: "label1".to_string(), color: LabelColor::purple()),
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
                children: vec![],
            }),
        ]); "TEST3: One message in inbox, other in folder")]
    #[test_case(
        &STARRED,
        vec![
            MessageWithLabels { message: message!(remote_id: msg_id!("message_1")), labels: vec![OUTBOX.clone()] },
            MessageWithLabels { message: message!(remote_id: msg_id!("message_2")), labels: vec![INBOX.clone()] },
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
        ]); "TEST4: One message in Inbox, other in Outbox when view is STARRED")]
    #[test_case(
            &CUSTOM_FOLDER,
            vec![
                MessageWithLabels { message: message!(remote_id: msg_id!("message_2")), labels: vec![CUSTOM_FOLDER.clone()] },
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
                    label_id: "0123".into(),
                    name: "My custom folder".into(),
                    children: vec![],
                }),
            ]); "TEST5: Message in custom folder when viewed from custom folder")]
    #[test_case(
        &INBOX,
        vec![
            MessageWithLabels { message: message!(remote_id: msg_id!("message_1")), labels: vec![
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
        messages: Vec<MessageWithLabels>,
        labels: Vec<Label>,
        expected: Result<&[ExpectedMoveAction], AppError>,
    ) {
        let stash = new_test_connection().await;
        let mut conn = stash.connection();
        let address = create_address(&mut conn).await;
        let mut conversation = conversation!(remote_id: conv_id!("conversation"));
        let mut message_ids = vec![];
        conn.tx::<_, _, StashError>(async |tx| {
            conversation.save(tx).await.unwrap();

            let mut settings = MailSettings::default();
            settings.save(tx).await.unwrap();

            for mut label in labels {
                label.save(tx).await.expect("failed to create label");
                MessageCounters::new(label.id())
                    .save(tx)
                    .await
                    .expect("failed to create message counters");
            }

            for MessageWithLabels {
                mut message,
                labels: message_labels,
            } in messages
            {
                message.local_address_id = address.id();
                message.remote_address_id = address.remote_id.clone().unwrap();
                message.local_conversation_id = conversation.local_id;
                message.remote_conversation_id = conversation.remote_id.clone();

                message.save(tx).await.expect("failed to create message");

                message_ids.push(message.id());

                for mut label in message_labels {
                    label.save(tx).await.expect("failed to create label");
                    let label_id = label.id();
                    ConversationCounters::new(label_id)
                        .save(tx)
                        .await
                        .expect("failed to create conversation counters");
                    MessageCounters::new(label_id)
                        .save(tx)
                        .await
                        .expect("failed to create message counters");

                    let ids = vec![message.id()];

                    Message::apply_label(label_id, ids, tx).await.unwrap();
                }
            }
            Ok(())
        })
        .await
        .unwrap();

        let new_conn = || stash.connection();
        let view = Label::find_by_remote_id(view.remote_id.clone().unwrap(), &conn)
            .await
            .unwrap()
            .unwrap();

        let result = Message::available_move_to_actions(view, message_ids, &conn).await;

        match result {
            Ok(actual) => {
                let actual = stream::iter(actual.into_iter())
                    .then(
                        |action| async move { ExpectedMoveAction::new(action, &new_conn()).await },
                    )
                    .collect::<Vec<_>>()
                    .await;

                assert_eq!(actual, expected.unwrap());
            }
            Err(err) => {
                assert_eq!(err.to_string(), expected.unwrap_err().to_string());
            }
        }
    }
}

#[tokio::test]
async fn test_create_message() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection();
    test_create_message_dependencies_core(&mut tether).await;
    let _conversation_id = test_create_message_dependencies(&mut tether).await;
    let message = test_message_with_metadata(vec![LabelId::inbox(), MY_LABEL_ID1.clone()], vec![]);
    let id =
        tether
            .tx::<_, _, StashError>(async |tx| {
                Ok(Message::create_or_update_messages_from_metadata(
                    vec![message.metadata.clone()],
                    tx,
                )
                .await
                .expect("failed to create message")
                .into_iter()
                .next()
                .unwrap())
            })
            .await
            .unwrap();
    let db_message = Message::load(id, &tether)
        .await
        .expect("failed to get message")
        .expect("must have a value");
    let (mut expected, _, _) = Message::from_api_data(message, &tether).await.unwrap();
    let label = Label::find_by_remote_id(MY_LABEL_ID1.clone(), &tether)
        .await
        .unwrap()
        .unwrap();
    resolve_local_ids(&tether, &mut expected).await;
    expected.local_id = Some(1.into());
    expected.row_id = Some(1_u64);
    expected.exclusive_location = ExclusiveLocation::new(
        &Label::find_by_remote_id(LabelId::inbox(), &tether)
            .await
            .unwrap()
            .unwrap(),
    );
    expected.custom_labels = vec![CustomLabel {
        local_id: label.id(),
        name: label.name,
        color: label.color,
    }];

    assert_eq!(db_message, expected);
    assert_eq!(db_message.label_ids.len(), 2);
}

#[tokio::test]
async fn test_create_message_without_synced_conversation() {
    // Validate that we can create messages without having fetch the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection();
    test_create_message_dependencies_core(&mut tether).await;
    create_labels(&mut tether).await;

    let api_metadata = test_message_metadata([MY_LABEL_ID1.clone()], []);
    let remote_id = api_metadata.id.clone();
    tether
        .tx::<_, _, StashError>(async |tx| {
            Message::create_or_update_messages_from_metadata(vec![api_metadata], tx)
                .await
                .expect("failed to create message");
            Ok(())
        })
        .await
        .unwrap();
    let db_metadata = Message::find_by_remote_id(remote_id, &tether)
        .await
        .expect("failed to get message")
        .expect("must have a value");

    // ensure we can't access this conversation
    let conv = Conversation::find_by_id(db_metadata.local_conversation_id.unwrap(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(!conv.is_known);
    assert_eq!(conv.remote_id, db_metadata.remote_conversation_id);

    // create the conversation
    let mut conversation: Conversation = test_conversation(
        [ApiConversationLabel {
            id: MY_LABEL_ID1.clone(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
        }],
        [],
    )
    .into();

    tether
        .tx::<_, _, StashError>(async |tx| {
            conversation
                .save(tx)
                .await
                .expect("failed to create conversation");
            Ok(())
        })
        .await
        .unwrap();

    let conv = Conversation::find_by_id(conversation.id(), &tether)
        .await
        .unwrap()
        .unwrap();
    assert!(conv.is_known);
    assert_eq!(conv.remote_id, db_metadata.remote_conversation_id);
}

#[tokio::test]
async fn test_create_message_with_attachments() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    test_create_message_dependencies_core(&mut conn).await;
    let attachment_metadata = ApiAttachmentMetadata {
        id: AttachmentId::from("myattachment"),
        size: 80,
        name: "foo.pdf".to_owned(),
        mime_type: attachment::MimeType::application_pdf().to_string(),
        disposition: ApiDisposition::Attachment,
    };
    let _ = test_create_message_dependencies(&mut conn).await;
    let message = test_message_with_metadata(
        vec![MY_LABEL_ID1.clone()],
        vec![attachment_metadata.clone()],
    );
    let id = conn
        .tx::<_, _, StashError>(async |tx| {
            Ok(
                Message::create_or_update_messages_from_metadata(vec![message.metadata], tx)
                    .await
                    .expect("failed to create message")
                    .into_iter()
                    .next()
                    .unwrap(),
            )
        })
        .await
        .unwrap();

    let db_message = Message::load(id, &conn)
        .await
        .expect("failed to get message")
        .expect("must have a value");
    assert_eq!(db_message.label_ids.len(), 1);
    assert_eq!(db_message.attachments_metadata.len(), 1);
}

// #[test]
// fn attachment_properly_initialized_after_conversation_load_chain() {
//     // * Create conversation with attachment
//     // * Create message with attachment
//     // * Create message body with attachment
//     // * Observe attachment is loaded correctly
//     with_file_sqlite_db(|mut core_conn, mut conn, _| {
//         with_tx_core(&mut core_conn, test_create_message_dependencies_core);
//         with_tx(&mut conn, |tx| {
//             let attachment_metadata = AttachmentMetadata {
//                 id: AttachmentId::from("myattachment"),
//                 size: 80,
//                 name: "foo.pdf".to_string(),
//                 mime_type: "application/pdf".to_string(),
//                 disposition: Disposition::Inline,
//             };
//             create_labels(tx);
//
//             let conversation = test_conversation(
//                 [ConversationLabels {
//                     id: MY_LABEL_ID1.clone(),
//                     context_num_unread: 0,
//                     context_num_messages: 0,
//                     context_time: 0,
//                     context_size: 0,
//                     context_num_attachments: 0,
//                     context_expiration_time: 0,
//                     context_snooze_time: 0,
//                 }],
//                 [attachment_metadata.clone()],
//             );
//
//             tx.create_conversation(&conversation).unwrap();
//
//             let metadata =
//                 test_message_metadata([MY_LABEL_ID1.clone()], [attachment_metadata.clone()]);
//             let id = tx
//                 .create_message_from_metadata(&metadata)
//                 .expect("failed to create message");
//
//             let message = Message {
//                 metadata,
//                 header: "".to_string(),
//                 parsed_headers: Default::default(),
//                 body: "".to_string(),
//                 mime_type: attachment::MimeType::TextPlain,
//                 attachments: vec![MessageAttachment {
//                     id: attachment_metadata.id.clone(),
//                     name: attachment_metadata.name.clone(),
//                     size: attachment_metadata.size,
//                     mime_type: attachment_metadata.mime_type,
//                     disposition: attachment_metadata.disposition,
//                     key_packets: KeyPackets::from(""),
//                     signature: None,
//                     enc_signature: None,
//                     headers: MessageAttachmentHeaders {
//                         content_disposition: "inline".to_owned(),
//                         content_id: None,
//                         content_transfer_encoding: None,
//                         image_width: None,
//                         image_height: None,
//                     },
//                 }],
//             };
//
//             tx.create_or_update_message_body(&message).unwrap();
//
//             let attachments = tx.attachments_for_message(id).unwrap();
//             assert_eq!(attachments.len(), 1);
//             let attachment = &attachments[0];
//             assert_eq!(attachment.address_id, message.metadata.address_id);
//             assert_eq!(attachment.message_id, Some(id));
//         });
//     });
// }

#[tokio::test]
async fn test_update_message() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection();
    tether.execute("DELETE FROM labels", vec![]).await.unwrap();
    test_create_message_dependencies_core(&mut tether).await;
    let _conv_id = test_create_message_dependencies(&mut tether).await;
    tether
        .tx::<_, _, StashError>(async |tx| test_starred_label().save(tx).await)
        .await
        .unwrap();
    let message = test_message_with_metadata(vec![MY_LABEL_ID1.clone()], vec![]);
    let mut metadata_updated =
        test_message_with_metadata(vec![MY_LABEL_ID2.clone(), LabelId::starred()], vec![]);
    metadata_updated.metadata.order = 20;
    metadata_updated.metadata.unread = true;
    metadata_updated
        .metadata
        .label_ids
        .push(LabelId::starred().clone());
    // This value contains unused flags.
    metadata_updated.metadata.flags = ApiMessageFlags::from_bits(8397841).unwrap();
    let id = tether
        .tx::<_, _, StashError>(async |tx| {
            Ok(
                Message::create_or_update_messages_from_metadata(vec![message.metadata], tx)
                    .await
                    .expect("failed to create message")
                    .into_iter()
                    .next()
                    .unwrap(),
            )
        })
        .await
        .unwrap();

    let mut db_message = Message::load(id, &tether)
        .await
        .expect("failed to get message")
        .expect("must have a value");
    db_message.display_order = metadata_updated.metadata.order;
    db_message.unread = metadata_updated.metadata.unread;
    db_message.label_ids = metadata_updated.metadata.label_ids.clone();
    db_message.flags = MessageFlags::from(metadata_updated.metadata.flags);
    tether
        .tx::<_, _, StashError>(async |tx| {
            db_message.save(tx).await.expect("failed to update message");
            Ok(())
        })
        .await
        .unwrap();

    let label = Label::find_by_remote_id(MY_LABEL_ID1.clone(), &tether)
        .await
        .unwrap()
        .unwrap();
    let (mut expected, _, _) = Message::from_api_data(metadata_updated, &tether)
        .await
        .unwrap();
    resolve_local_ids(&tether, &mut expected).await;
    expected.custom_labels = vec![CustomLabel {
        local_id: label.id(),
        name: label.name,
        color: label.color,
    }];
    expected.local_id = Some(1.into());
    expected.row_id = Some(1);
    assert_eq!(db_message, expected);
    assert!(db_message.is_starred());
    assert_eq!(db_message.label_ids.len(), 3);
    let db_message = Message::load(id, &tether)
        .await
        .expect("failed to get message")
        .expect("must have a value");
    assert!(db_message.is_starred());
    assert_eq!(db_message.label_ids.len(), 2);
}

#[tokio::test]
#[ignore]
async fn test_message_counts() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection();
    create_address(&mut tether).await;
    let labels = create_labels(&mut tether).await;
    let counts = vec![
        MessageLabelsCount {
            label_id: MY_LABEL_ID1.clone(),
            total: 20,
            unread: 4,
        },
        MessageLabelsCount {
            label_id: MY_LABEL_ID2.clone(),
            total: 400,
            unread: 124,
        },
    ];

    tether
        .tx::<_, _, StashError>(async |tx| {
            MessageLabelsCount::create_or_update_message_counts(counts.clone(), tx)
                .await
                .expect("failed to creat counters");
            Ok(())
        })
        .await
        .unwrap();
    let db_labels = Label::all(&tether).await.expect("failed to get counters");
    let db_counters = MessageCounters::all(&tether)
        .await
        .expect("failed to get counters");
    let db_counters = futures::stream::FuturesOrdered::from_iter(db_counters.iter().map({
        let tether = &tether;
        move |c| async move {
            c.message_count(tether)
                .await
                .expect("failed to get message count")
        }
    }))
    .collect::<Vec<_>>()
    .await;
    assert!(db_counters.contains(&counts[0]));
    assert!(db_counters.contains(&counts[1]));

    let label_msg_count = Label::load(labels[0], &tether).await.unwrap().unwrap();
    assert!(db_labels.contains(&label_msg_count));

    assert_eq!(db_labels.len(), 1);
    assert_eq!(db_labels[0].remote_id, Some(counts[0].label_id.clone()));
}

#[tokio::test]
pub async fn test_delete_local_message() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    // Deleting a message must
    // * Update conversation counters
    // * Update conversation labels
    // * Update message counters
    let (mut state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    {
        // Delete 3rd message from 1st conversation.
        let message = &mut state.messages[2];
        let local_id = *state_map
            .messages
            .get(&message.remote_id.clone().unwrap())
            .unwrap();

        conn.tx::<_, _, AppError>(async |tx| Message::mark_deleted(vec![local_id], tx).await)
            .await
            .unwrap();

        let conv_counts = conv_counts_as_map(&conn).await;
        let msg_counts = msg_counts_as_map(&conn).await;

        for label in message
            .label_ids
            .iter_mut()
            .filter(|l| *l != &SystemLabel::AllMail.label_id())
        {
            let local_label_id = *state_map
                .labels
                .get(label)
                .expect("Failed to resolve label");
            let conv_count = conv_counts.get(&local_label_id).unwrap();
            let start_conv_count = state_map.conversation_counts.get(label).unwrap();
            let start_msg_count = state_map.message_counts.get(label).unwrap();
            let local_conv = ContextualConversation::new(
                Conversation::load(local_conv_id, &conn)
                    .await
                    .unwrap()
                    .unwrap(),
                local_label_id,
            )
            .unwrap();

            let remote_conversation_label = find_conversation_label(&state.conversations[0], label);

            assert_eq!(
                local_conv.num_messages,
                remote_conversation_label.context_num_messages - 1
            );
            assert_eq!(
                local_conv.num_unread,
                remote_conversation_label.context_num_unread - 1
            );
            assert_eq!(
                local_conv.size,
                remote_conversation_label.context_size - message.size
            );
            assert_eq!(
                local_conv.num_attachments,
                remote_conversation_label.context_num_attachments - message.num_attachments as u64
            );

            let local_conv = Conversation::load(local_conv_id, &conn)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                local_conv.num_messages,
                state.conversations[0].num_messages - 1
            );

            assert_eq!(
                local_conv.num_messages,
                state.conversations[0].num_messages - 1
            );
            assert_eq!(local_conv.num_unread, state.conversations[0].num_unread - 1);

            let msg_count = msg_counts.get(&local_label_id).unwrap();
            assert_eq!(msg_count.total, start_msg_count.total - 1);
            assert_eq!(msg_count.unread, start_msg_count.unread - 1);

            assert_eq!(conv_count.total, start_conv_count.total);
            // Conversation 1 & 2 have two unread message each on different labels and we removed
            // the unread message from label1.
            assert_eq!(conv_count.unread, 1);
        }
    }

    {
        // Delete remaining messages from first conversation
        let ids = state
            .messages
            .iter()
            .filter(|m| m.remote_conversation_id == state.conversations[0].remote_id)
            .map(|m| {
                *state_map
                    .messages
                    .get(&m.remote_id.clone().unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();

        conn.tx::<_, _, AppError>(async |tx| Message::mark_deleted(ids, tx).await)
            .await
            .unwrap();

        let conv_counts = conv_counts_as_map(&conn).await;
        let msg_counts = msg_counts_as_map(&conn).await;

        for label in &state.conversations[0].labels {
            let local_label_id = *state_map
                .labels
                .get(&label.remote_label_id.clone().unwrap())
                .expect("Failed to resolve label");
            let conv_count = conv_counts.get(&local_label_id).unwrap();
            let msg_count = msg_counts.get(&local_label_id).unwrap();
            let start_conv_count = state_map
                .conversation_counts
                .get(&label.remote_label_id.clone().unwrap())
                .unwrap();
            let start_msg_count = state_map
                .message_counts
                .get(&label.remote_label_id.clone().unwrap())
                .unwrap();

            // Conversation should no longer exist
            assert_eq!(conv_count.total, start_conv_count.total - 1);
            if label.remote_label_id == Some(SystemLabel::AllMail.label_id()) {
                assert_eq!(msg_count.total, start_msg_count.total - 4);
            } else {
                assert_eq!(msg_count.total, start_msg_count.total - 2);
            }
        }

        // Conversation should be deleted
        assert!(
            Conversation::load(local_conv_id, &conn)
                .await
                .unwrap()
                .unwrap()
                .deleted
        );

        assert!(
            Conversation::find(
                "WHERE local_id = ? AND deleted = 0",
                params![local_conv_id],
                &conn,
            )
            .await
            .unwrap()
            .is_empty()
        );
    }
}

#[tokio::test]
#[ignore]
pub async fn test_message_metadata_list() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (_, _state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;
    let messages = Message::all(&conn).await.expect("failed to get messages");
    assert_eq!(messages.len(), 3);
    assert!(messages[0].time > messages[1].time);
    assert!(messages[1].time > messages[2].time);
}

#[tokio::test]
#[ignore]
pub async fn test_delete_local_message_does_not_change_conv_unread_count() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (mut state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    // Delete 2nd message from 1st conversation.
    let message = &mut state.messages[0];
    let _local_id = *state_map
        .messages
        .get(&message.remote_id.clone().unwrap())
        .unwrap();
    message.deleted = true;
    conn.tx::<_, _, StashError>(async |tx| {
        message
            .save(tx)
            .await
            .expect("failed to mark local message as deleted");
        Ok(())
    })
    .await
    .unwrap();
    let local_label_id = state_map.labels.get(&MY_LABEL_ID1).unwrap();

    let conv_counts = conv_counts_as_map(&conn).await;
    let label_conv_counts = conv_counts.get(local_label_id).unwrap();
    assert_eq!(label_conv_counts.unread, 1);
}

#[tokio::test]
pub async fn test_undelete_local_message() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    // Same as test_delete_local_message, but undo the operations
    let (mut state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(&state.conversations[0].remote_id.clone().unwrap())
        .unwrap();
    {
        // Delete 3rd message from 1st conversation.
        let message = &mut state.messages[2];
        let local_id = *state_map
            .messages
            .get(&message.remote_id.clone().unwrap())
            .unwrap();

        conn.tx::<_, _, StashError>(async |tx| {
            Message::mark_deleted(vec![local_id], tx).await.unwrap();
            Message::mark_undeleted(vec![local_id], tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

        let conv_counts = conv_counts_as_map(&conn).await;
        let msg_counts = msg_counts_as_map(&conn).await;

        for label in &mut message.label_ids {
            let local_label_id = *state_map
                .labels
                .get(label)
                .expect("Failed to resolve label");
            let conv_count = conv_counts.get(&local_label_id).unwrap();
            let start_conv_count = state_map.conversation_counts.get(label).unwrap();
            let start_msg_count = state_map.message_counts.get(label).unwrap();

            let local_conv = ContextualConversation::new(
                Conversation::load(local_conv_id, &conn)
                    .await
                    .unwrap()
                    .unwrap(),
                local_label_id,
            )
            .unwrap();
            let remote_conversation_label = find_conversation_label(&state.conversations[0], label);

            assert_eq!(
                local_conv.num_messages,
                remote_conversation_label.context_num_messages,
            );
            assert_eq!(
                local_conv.num_unread,
                remote_conversation_label.context_num_unread,
            );
            assert_eq!(local_conv.size, remote_conversation_label.context_size,);
            assert_eq!(
                local_conv.num_attachments,
                remote_conversation_label.context_num_attachments,
            );

            let local_conv = Conversation::load(local_conv_id, &conn)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(local_conv.num_messages, state.conversations[0].num_messages,);
            assert_eq!(local_conv.num_unread, state.conversations[0].num_unread);

            let msg_count = msg_counts.get(&local_label_id).unwrap();
            assert_eq!(msg_count.total, start_msg_count.total);
            assert_eq!(msg_count.unread, start_msg_count.unread);

            assert_eq!(conv_count.total, start_conv_count.total);
            assert_eq!(conv_count.unread, start_conv_count.unread);
        }
    }

    {
        // Delete all messages from first conversation and restore
        let ids = state
            .messages
            .iter()
            .filter(|m| m.remote_conversation_id == state.conversations[0].remote_id.clone())
            .map(|m| {
                *state_map
                    .messages
                    .get(&m.remote_id.clone().unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        conn.tx::<_, _, StashError>(async |tx| {
            for id in &ids {
                let mut message = Message::load(*id, tx)
                    .await
                    .expect("failed to get message")
                    .expect("must have a value");
                message.deleted = true;
                message
                    .save(tx)
                    .await
                    .expect("failed to mark local message as deleted");
            }
            for id in &ids {
                let mut message = Message::load(*id, tx)
                    .await
                    .expect("failed to get message")
                    .expect("must have a value");
                message.deleted = false;
                message
                    .save(tx)
                    .await
                    .expect("failed to mark local message as deleted");
            }
            Ok(())
        })
        .await
        .unwrap();

        let conv_counts = conv_counts_as_map(&conn).await;
        let msg_counts = msg_counts_as_map(&conn).await;

        for label in &state.conversations[0].labels {
            let local_label_id = *state_map
                .labels
                .get(&label.remote_label_id.clone().unwrap())
                .expect("Failed to resolve label");
            let conv_count = conv_counts.get(&local_label_id).unwrap();
            let msg_count = msg_counts.get(&local_label_id).unwrap();
            let start_conv_count = state_map
                .conversation_counts
                .get(&label.remote_label_id.clone().unwrap())
                .unwrap();
            let start_msg_count = state_map
                .message_counts
                .get(&label.remote_label_id.clone().unwrap())
                .unwrap();

            // Conversation should no longer exist
            assert_eq!(conv_count.total, start_conv_count.total);
            assert_eq!(msg_count.total, start_msg_count.total);
        }

        // Conversation should be deleted
        assert!(
            Conversation::load(local_conv_id, &conn)
                .await
                .unwrap()
                .is_some()
        );
    }
}

#[tokio::test]
async fn test_create_message_and_body() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    test_create_message_dependencies_core(&mut conn).await;
    test_create_message_dependencies(&mut conn).await;
    let message = ApiMessage {
        metadata: test_message_metadata(vec![MY_LABEL_ID1.clone()], vec![]),
        body: ApiMessageBody {
            header: "my headers".to_owned(),
            parsed_headers: hash_map! {
                "foo".to_owned(): serde_json::Value::String("bar".to_owned()),
                "zeta".to_owned(): serde_json::Value::String("gama".to_owned()),
            },
            body: "my_message".to_owned(),
            mime_type: ApiMimeType::TextPlain,
            attachments: vec![],
        },
    };
    let (mut metadata, mut body_metadata, _) = Message::from_api_data(message.clone(), &conn)
        .await
        .unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        metadata.save(tx).await.expect("failed to create message");
        body_metadata
            .save(tx)
            .await
            .expect("failed to store message body metadata in db");
        Ok(())
    })
    .await
    .unwrap();
    let db_message = Message::load(metadata.id(), &conn)
        .await
        .expect("failed to get message")
        .expect("must have a value");

    assert_eq!(metadata.id(), body_metadata.local_message_id.unwrap());

    let db_message_body = MessageBodyMetadata::load(metadata.id(), &conn)
        .await
        .expect("failed to get message body")
        .expect("must have a value");

    assert_eq!(body_metadata, db_message_body);

    let expected = MessageBodyMetadata {
        local_message_id: db_message.local_id,
        remote_message_id: db_message.remote_id.clone(),
        header: message.body.header.clone(),
        parsed_headers: ParsedHeaders {
            headers: message.body.parsed_headers.clone(),
        },
        mime_type: message.body.mime_type.into(),
        attachments: vec![],
        row_id: Some(1),
    };

    assert_eq!(db_message_body, expected);
}

#[tokio::test]
async fn test_update_message_and_body() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    test_create_message_dependencies_core(&mut conn).await;
    test_create_message_dependencies(&mut conn).await;

    let mut message = ApiMessage {
        metadata: test_message_metadata(vec![MY_LABEL_ID1.clone()], vec![]),
        body: ApiMessageBody {
            header: "my headers".to_owned(),
            parsed_headers: hash_map! {
                "foo".to_owned(): serde_json::Value::String("bar".to_owned()),
                "zeta".to_owned(): serde_json::Value::String("gama".to_owned()),
            },
            body: "my_message".to_owned(),
            mime_type: ApiMimeType::TextPlain,
            attachments: vec![],
        },
    };

    let (mut metadata, mut body_metadata, _) = Message::from_api_data(message.clone(), &conn)
        .await
        .unwrap();
    conn.tx::<_, _, StashError>(async |tx| {
        metadata.save(tx).await.expect("failed to create message");

        body_metadata
            .save(tx)
            .await
            .expect("failed to store message body metadata in db");
        Ok(())
    })
    .await
    .unwrap();
    let id = metadata.id();

    let db_message = Message::load(id, &conn)
        .await
        .expect("failed to get message")
        .expect("must have a value");

    message
        .body
        .parsed_headers
        .insert("marco".to_owned(), json!("polo"));

    conn.tx::<_, _, StashError>(async |tx| {
        MessageBodyMetadata {
            parsed_headers: ParsedHeaders {
                headers: message.body.parsed_headers.clone(),
            },
            mime_type: MimeType::TextHtml,
            header: "new header".to_string(),
            ..body_metadata
        }
        .save(tx)
        .await
    })
    .await
    .unwrap();

    let db_message_body = MessageBodyMetadata::load(id, &conn)
        .await
        .expect("failed to get message body")
        .expect("must have a value");

    let expected = MessageBodyMetadata {
        local_message_id: db_message.local_id,
        remote_message_id: db_message.remote_id.clone(),
        header: "new header".to_string(),
        parsed_headers: ParsedHeaders {
            headers: message.body.parsed_headers,
        },
        mime_type: MimeType::TextHtml,
        attachments: vec![],
        row_id: Some(1),
    };

    assert_eq!(db_message_body, expected);
}

#[tokio::test]
async fn test_create_message_and_body_with_attachments() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    test_create_message_dependencies_core(&mut conn).await;
    let attachment_id = AttachmentId::from("attachment");
    test_create_message_dependencies(&mut conn).await;
    let message = ApiMessage {
        metadata: test_message_metadata(
            vec![MY_LABEL_ID1.clone()],
            vec![ApiAttachmentMetadata {
                id: attachment_id.clone(),
                size: 1024,
                name: "fooo".to_owned(),
                mime_type: attachment::MimeType::text_html().to_string(),
                disposition: ApiDisposition::Attachment,
            }],
        ),
        body: ApiMessageBody {
            header: "my headers".to_owned(),
            parsed_headers: hash_map! {
                "foo".to_owned(): serde_json::Value::String("bar".to_owned()),
                "zeta".to_owned(): serde_json::Value::String("gama".to_owned()),
            },
            body: "my_message".to_owned(),
            mime_type: ApiMimeType::TextPlain,
            attachments: vec![ApiMessageAttachment {
                id: attachment_id.clone(),
                name: "fooo".to_owned(),
                size: 1024,
                mime_type: attachment::MimeType::text_html().to_string(),
                disposition: ApiDisposition::Attachment,
                key_packets: KeyPackets::from("packets"),
                signature: None,
                enc_signature: None,
                headers: ApiMessageAttachmentHeaders {
                    content_disposition: "inline".to_owned(),
                    content_id: Some("mycontent_id".to_owned()),
                    content_transfer_encoding: Some("base64".to_owned()),
                    image_width: Some("1280".to_owned()),
                    image_height: Some("720".to_owned()),
                },
            }],
        },
    };

    let (mut metadata, mut body_metadata, _) = Message::from_api_data(message.clone(), &conn)
        .await
        .unwrap();

    conn.tx::<_, _, StashError>(async |tx| {
        metadata.save(tx).await.expect("failed to create message");
        body_metadata.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    let id = metadata.id();

    let db_message = Message::load(id, &conn)
        .await
        .expect("failed to get message")
        .expect("must have a value");

    let local_attachment = message.body.attachments.first().unwrap();

    assert_eq!(
        local_attachment.headers.content_id,
        message.body.attachments[0].headers.content_id
    );
    assert_eq!(
        local_attachment.headers.content_transfer_encoding,
        message.body.attachments[0]
            .headers
            .content_transfer_encoding
    );
    assert_eq!(
        local_attachment.headers.image_width,
        message.body.attachments[0].headers.image_width
    );
    assert_eq!(
        local_attachment.headers.image_height,
        message.body.attachments[0].headers.image_height
    );

    let new_metadata = MessageBodyMetadata::for_message(db_message.local_id.unwrap(), &conn)
        .await
        .unwrap()
        .unwrap();
    let attachment =
        Attachment::find_by_id(db_message.attachments_metadata[0].local_id.unwrap(), &conn)
            .await
            .unwrap()
            .unwrap();

    assert_eq!(new_metadata.attachments.len(), 1);
    assert_eq!(attachment, new_metadata.attachments[0]);
}

#[tokio::test]
async fn message_metadata_update_does_not_purge_inline_attachments() {
    // Ensure that metadata updates do not wipe inline attachments as metadata only
    // has attachments with disposition attachment.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    test_create_message_dependencies_core(&mut conn).await;
    let attachment_id = AttachmentId::from("attachment");
    let attachment_inline_id = AttachmentId::from("attachment-inine");
    test_create_message_dependencies(&mut conn).await;
    let mut message = ApiMessage {
        metadata: test_message_metadata(
            vec![MY_LABEL_ID1.clone()],
            vec![ApiAttachmentMetadata {
                id: attachment_id.clone(),
                size: 1024,
                name: "fooo".to_owned(),
                mime_type: attachment::MimeType::text_html().to_string(),
                disposition: ApiDisposition::Attachment,
            }],
        ),
        body: ApiMessageBody {
            header: "my headers".to_owned(),
            parsed_headers: hash_map! {
                "foo".to_owned(): serde_json::Value::String("bar".to_owned()),
                "zeta".to_owned(): serde_json::Value::String("gama".to_owned()),
            },
            body: "my_message".to_owned(),
            mime_type: ApiMimeType::TextPlain,
            attachments: vec![
                ApiMessageAttachment {
                    id: attachment_id.clone(),
                    name: "fooo".to_owned(),
                    size: 1024,
                    mime_type: attachment::MimeType::text_html().to_string(),
                    disposition: ApiDisposition::Attachment,
                    key_packets: KeyPackets::from("packets"),
                    signature: None,
                    enc_signature: None,
                    headers: ApiMessageAttachmentHeaders {
                        content_disposition: "attachment".to_owned(),
                        content_id: None,
                        content_transfer_encoding: Some("base64".to_owned()),
                        image_width: None,
                        image_height: None,
                    },
                },
                ApiMessageAttachment {
                    id: attachment_inline_id.clone(),
                    name: "image.png".to_owned(),
                    size: 1024,
                    mime_type: "image/png".to_owned(),
                    disposition: ApiDisposition::Inline,
                    key_packets: KeyPackets::from("packets"),
                    signature: None,
                    enc_signature: None,
                    headers: ApiMessageAttachmentHeaders {
                        content_disposition: "inline".to_owned(),
                        content_id: Some("mycontent_id".to_owned()),
                        content_transfer_encoding: Some("base64".to_owned()),
                        image_width: Some("1280".to_owned()),
                        image_height: Some("720".to_owned()),
                    },
                },
            ],
        },
    };

    message.metadata.num_attachments = 2;

    let (mut metadata, mut body_metadata, _) = Message::from_api_data(message.clone(), &conn)
        .await
        .unwrap();

    conn.tx::<_, _, StashError>(async |tx| {
        metadata.save(tx).await.expect("failed to create message");
        body_metadata.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    let id = metadata.id();

    let db_message = Message::load(id, &conn)
        .await
        .expect("failed to get message")
        .expect("must have a value");

    assert_eq!(db_message.num_attachments, 2);
    assert_eq!(db_message.attachments_metadata.len(), 1);
    assert_eq!(
        db_message.attachments_metadata[0].remote_id(),
        Some(attachment_id.clone())
    );
    assert_eq!(
        db_message.attachments_metadata[0].disposition,
        Disposition::Attachment
    );

    let db_body_metadata = MessageBodyMetadata::for_message(db_message.id(), &conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(db_body_metadata.attachments.len(), 2);

    // save message again to simulate event loop update
    conn.tx::<_, _, StashError>(async |tx| {
        metadata.save(tx).await.expect("failed to create message");
        Ok(())
    })
    .await
    .unwrap();

    // Inline attachment should not go missing.
    let db_body_metadata = MessageBodyMetadata::for_message(db_message.id(), &conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(db_body_metadata.attachments.len(), 2);
}

#[tokio::test]
async fn messages_mark_read() {
    // Mark conversation as read and update all conversation / message counts
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id1 = *state_map
        .messages
        .get(state.messages[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id3 = *state_map
        .messages
        .get(state.messages[2].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id4 = *state_map
        .messages
        .get(state.messages[3].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();

    let check_counters = |stash: Stash,
                          read_message_count: u64,
                          read_conv_count: u64|
     -> BoxFuture<'_, ()> {
        let state_map = &state_map;
        async move {
            let clouser_conn = stash.connection();
            // Check conversation counts
            {
                let conv_counts = conv_counts_as_map(&clouser_conn).await;
                // Check conversation label1 values, values should be unchanged.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(
                        label_counts.unread,
                        start_label_counts.unread - read_conv_count
                    );

                    assert_eq!(label_counts.total, start_label_counts.total);
                }
                // Check conversation label2 values - should be unchanged.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = conv_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
            }

            // Check message counts
            {
                let message_counts = msg_counts_as_map(&clouser_conn).await;

                // Check label1
                {
                    let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = message_counts.get(&local_label_id1).unwrap();
                    assert_eq!(
                        label_counts.unread,
                        start_label_counts.unread - read_message_count
                    );
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
        .boxed()
    };

    conn.tx::<_, _, StashError>(async |tx| {
        Message::mark_read([local_msg_id1], tx)
            .await
            .expect("failed to mark as read");
        Ok(())
    })
    .await
    .unwrap();
    let db_message = Message::find_by_id(local_msg_id1, &conn)
        .await
        .expect("failed to get message")
        .unwrap();

    // Msg is read.
    assert!(!db_message.unread);

    let db_conv = ContextualConversation::new(
        Conversation::find_by_id(local_conv_id, &conn)
            .await
            .unwrap()
            .unwrap(),
        local_label_id1,
    )
    .unwrap();
    assert_eq!(db_conv.num_unread, 2);
    let stash_fun = || stash.clone();

    check_counters(stash_fun(), 1, 0).await;
    conn.tx::<_, _, StashError>(async |tx| {
        Message::mark_read(std::iter::once(local_msg_id3), tx)
            .await
            .expect("failed to mark as read");
        Ok(())
    })
    .await
    .unwrap();
    check_counters(stash_fun(), 2, 0).await;
    conn.tx::<_, _, StashError>(async |tx| {
        Message::mark_read(std::iter::once(local_msg_id4), tx)
            .await
            .expect("failed to mark as read");
        Ok(())
    })
    .await
    .unwrap();
    // All conversation messages on label_1 have been marked as read, we should now see an updated
    // conversation count.
    check_counters(stash_fun(), 3, 1).await;

    let db_conv = ContextualConversation::new(
        Conversation::find_by_id(local_conv_id, &conn)
            .await
            .unwrap()
            .unwrap(),
        local_label_id1,
    )
    .unwrap();
    assert_eq!(db_conv.num_unread, 0);
}

#[tokio::test]
async fn messages_mark_unread() {
    // Mark conversation as read and update all conversation / message counts
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    let mut state = new_test_unread_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id1 = *state_map
        .messages
        .get(state.messages[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id3 = *state_map
        .messages
        .get(state.messages[2].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id4 = *state_map
        .messages
        .get(state.messages[3].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
    let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();

    conn.tx::<_, _, StashError>(async |tx| {
        // mark messages read (also servers as bulk test).
        Message::mark_read([local_msg_id1, local_msg_id3, local_msg_id4], tx)
            .await
            .expect("failed to mark as read");
        Ok(())
    })
    .await
    .unwrap();

    let check_counters = |stash: Stash,
                          label_1_msg_diff: u64,
                          label_1_conv_diff: u64|
     -> BoxFuture<'_, ()> {
        let state_map = &state_map;
        let conn = stash.connection();
        async move {
            // Check conversation counts
            {
                let conv_counts = conv_counts_as_map(&conn).await;
                // Check conversation label1 values, values should be unchanged.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(
                        label_counts.unread,
                        start_label_counts.unread - label_1_conv_diff
                    );
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
                // Check conversation label2 values - should be unchanged.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = conv_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
            }

            // Check message counts
            {
                let message_counts = msg_counts_as_map(&conn).await;

                // Check label1
                {
                    let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = message_counts.get(&local_label_id1).unwrap();
                    assert_eq!(
                        label_counts.unread,
                        start_label_counts.unread - label_1_msg_diff
                    );
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
        .boxed()
    };

    check_counters(stash.clone(), 3, 1).await;
    conn.tx::<_, _, StashError>(async |tx| {
        Message::mark_unread(std::iter::once(local_msg_id1), tx)
            .await
            .expect("failed to mark as read");
        Ok(())
    })
    .await
    .unwrap();
    let db_message = Message::find_by_id(local_msg_id1, &conn)
        .await
        .unwrap()
        .unwrap();
    // Msg is unread.
    assert!(db_message.unread);

    let db_conv = ContextualConversation::new(
        Conversation::find_by_id(local_conv_id, &conn)
            .await
            .unwrap()
            .unwrap(),
        local_label_id1,
    )
    .unwrap();
    assert_eq!(db_conv.num_unread, 1);

    check_counters(stash.clone(), 2, 0).await;
    conn.tx::<_, _, StashError>(async |tx| {
        Message::mark_unread(std::iter::once(local_msg_id3), tx)
            .await
            .expect("failed to mark as read");
        Ok(())
    })
    .await
    .unwrap();
    check_counters(stash.clone(), 1, 0).await;
    conn.tx::<_, _, StashError>(async |tx| {
        Message::mark_unread(std::iter::once(local_msg_id4), tx)
            .await
            .expect("failed to mark as read");
        Ok(())
    })
    .await
    .unwrap();
    // All conversation messages on label_1 have been marked as read, we should now see an updated
    // conversation count.
    check_counters(stash.clone(), 0, 0).await;

    let db_conv = ContextualConversation::new(
        Conversation::find_by_id(local_conv_id, &conn)
            .await
            .unwrap()
            .unwrap(),
        local_label_id1,
    )
    .unwrap();
    assert_eq!(db_conv.num_unread, 3);
}

#[tokio::test]
async fn label_messages() {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id1 = *state_map
        .messages
        .get(state.messages[0].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id2 = *state_map
        .messages
        .get(state.messages[1].remote_id.as_ref().unwrap())
        .unwrap();
    let local_msg_id3 = *state_map
        .messages
        .get(state.messages[2].remote_id.as_ref().unwrap())
        .unwrap();
    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();

    conn.tx::<_, _, StashError>(async |tx| {
        Message::apply_label(local_label_id1, std::iter::once(local_msg_id1), tx)
            .await
            .expect("failed to label");
        Ok(())
    })
    .await
    .unwrap();

    let db_conversation = ContextualConversation::load(local_conv_id, local_label_id1, &conn)
        .await
        .expect("failed to get conversation")
        .unwrap();

    // There should be no unread messages.
    assert_eq!(db_conversation.num_unread, 0);
    assert_eq!(db_conversation.num_messages, 1);
    assert_eq!(db_conversation.num_attachments, 1);
    assert_eq!(db_conversation.size, state.messages[0].size,);
    assert_eq!(db_conversation.time, state.messages[0].time,);
    assert_eq!(
        db_conversation.expiration_time,
        state.messages[0].expiration_time,
    );
    assert_eq!(db_conversation.snooze_time, state.messages[0].snooze_time,);

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&conn).await;
        let label_counts = conv_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts.
    {
        let message_counts = msg_counts_as_map(&conn).await;
        let label_counts = message_counts.get(&local_label_id1).unwrap();
        assert_eq!(label_counts.unread, 0);
        assert_eq!(label_counts.total, 1);
    }

    let check_full_conversations = |stash: &Stash| -> BoxFuture<'_, ()> {
        let state = &state;
        let tether = stash.connection();
        async move {
            // Check conversation after all messages have been labeled.
            let db_conversation =
                ContextualConversation::load(local_conv_id, local_label_id1, &tether)
                    .await
                    .expect("failed to get conversation")
                    .unwrap();
            assert_eq!(db_conversation.num_unread, 1);
            assert_eq!(db_conversation.num_messages, 3);
            assert_eq!(db_conversation.num_attachments, 1);
            assert_eq!(
                db_conversation.size,
                state.messages.iter().fold(0, |x, m| x + m.size)
            );
            assert_eq!(
                db_conversation.time,
                state
                    .messages
                    .iter()
                    .fold(UnixTimestamp::new(0), |x, m| x.max(m.time))
            );
            assert_eq!(
                db_conversation.expiration_time,
                state
                    .messages
                    .iter()
                    .fold(UnixTimestamp::new(0), |x, m| x.max(m.expiration_time))
            );
            assert_eq!(
                db_conversation.snooze_time,
                state
                    .messages
                    .iter()
                    .fold(UnixTimestamp::new(0), |x, m| x.max(m.snooze_time))
            );

            // Check conversation counts.
            {
                let conv_counts = conv_counts_as_map(&tether).await;
                let label_counts = conv_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 1);
                assert_eq!(label_counts.total, 1);
            }

            // Check message counts.
            {
                let message_counts = msg_counts_as_map(&tether).await;
                let label_counts = message_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 1);
                assert_eq!(label_counts.total, 3);
            }
        }
        .boxed()
    };

    // Label remaining messages.
    conn.tx::<_, _, StashError>(async |tx| {
        Message::apply_label(local_label_id1, [local_msg_id2, local_msg_id3], tx)
            .await
            .unwrap();
        Ok(())
    })
    .await
    .unwrap();

    check_full_conversations(&stash).await;

    // Apply again, should be noop.
    conn.tx::<_, _, StashError>(async |tx| {
        Message::apply_label(
            local_label_id1,
            [local_msg_id1, local_msg_id2, local_msg_id3],
            tx,
        )
        .await
        .unwrap();
        Ok(())
    })
    .await
    .unwrap();

    check_full_conversations(&stash).await;
}

#[tokio::test]
async fn unlabel_messages() {
    // assign a label to messages and progressively remove it.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut tether, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut tether, state.clone()).await;

    let conv = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    let msg1 = *state_map
        .messages
        .get(state.messages[0].remote_id.as_ref().unwrap())
        .unwrap();
    let msg2 = *state_map
        .messages
        .get(state.messages[1].remote_id.as_ref().unwrap())
        .unwrap();
    let msg3 = *state_map
        .messages
        .get(state.messages[2].remote_id.as_ref().unwrap())
        .unwrap();
    let label = *state_map.labels.get(&MY_LABEL_ID1).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            Message::apply_label(label, [msg1, msg2, msg3], tx)
                .await
                .expect("failed to label");

            // unlabel first message.
            Message::remove_label(label, [msg1], tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();
    let msg1_remote = state.messages[0].remote_id.clone().unwrap();

    let db_conversation = ContextualConversation::load(conv, label, &tether)
        .await
        .expect("failed to get conversation")
        .unwrap();

    let curr_msgs = state
        .messages
        .iter()
        .filter(|m| m.remote_id.as_ref() != Some(&msg1_remote));

    // Check conversation status.
    assert_eq!(db_conversation.num_unread, 1);
    assert_eq!(db_conversation.num_messages, 2);
    assert_eq!(
        db_conversation.num_messages,
        curr_msgs.clone().count() as u64
    );
    assert_eq!(
        db_conversation.num_unread,
        curr_msgs.clone().filter(|m| m.unread).count() as u64
    );
    assert_eq!(db_conversation.num_attachments, 0);
    assert_eq!(
        db_conversation.size,
        curr_msgs.clone().map(|m| m.size).sum::<u64>()
    );
    assert_eq!(
        db_conversation.time,
        curr_msgs.clone().map(|m| m.time).max().unwrap()
    );
    assert_eq!(
        db_conversation.expiration_time,
        curr_msgs.clone().map(|m| m.expiration_time).max().unwrap()
    );
    assert_eq!(
        db_conversation.snooze_time,
        curr_msgs.clone().map(|m| m.snooze_time).max().unwrap()
    );

    // Check conversation counts have the new conversation.
    {
        let conv_counts = conv_counts_as_map(&tether).await;
        let label_counts = conv_counts.get(&label).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 1);
    }

    // Check message counts.
    {
        let message_counts = msg_counts_as_map(&tether).await;
        let label_counts = message_counts.get(&label).unwrap();
        assert_eq!(label_counts.unread, 1);
        assert_eq!(label_counts.total, 2);
    }

    let check_final_conv_state = async |tether: &Tether| {
        assert_eq!(
            ContextualConversation::load(conv, label, tether)
                .await
                .unwrap(),
            None,
            "Conversation should no longer have the label"
        );

        // Check conversation counts.
        {
            let conv_counts = conv_counts_as_map(tether).await;
            let label_counts = conv_counts.get(&label).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 0);
        }

        // Check message counts.
        {
            let message_counts = msg_counts_as_map(tether).await;
            let label_counts = message_counts.get(&label).unwrap();
            assert_eq!(label_counts.unread, 0);
            assert_eq!(label_counts.total, 0);
        }
    };

    // remove labels
    tether
        .tx::<_, _, StashError>(async |tx| {
            Message::remove_label(label, [msg2, msg3], tx)
                .await
                .unwrap();
            Ok(())
        })
        .await
        .unwrap();

    check_final_conv_state(&tether).await;

    // Apply again, should be noop.
    tether
        .tx::<_, _, StashError>(async |tx| {
            Message::remove_label(label, [msg1, msg2, msg3], tx)
                .await
                .unwrap();
            Ok(())
        })
        .await
        .unwrap();

    check_final_conv_state(&tether).await;
}

pub(super) static MY_MESSAGE_ID: LazyLock<MessageId> =
    LazyLock::new(|| MessageId::from("MyRemoteId"));

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
async fn exclusive_location_from_api_metadata(
    mut labels: Vec<Label>,
    expected: Option<(bool, &str)>,
) {
    // Setup
    //   * Create a ApiMessageMetadata with label_ids

    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    test_create_message_dependencies_core(&mut conn).await;

    conn.tx::<_, _, StashError>(async |tx| {
        for label in &mut labels {
            label.save(tx).await.unwrap();
        }
        Ok(())
    })
    .await
    .unwrap();

    let label_ids = labels.iter().map(|l| l.remote_id.clone().unwrap());
    let api_metadata = test_message_metadata(label_ids, vec![]);

    // Action
    let result = Message::from_api_metadata(api_metadata, &conn)
        .await
        .unwrap();

    // Validation
    if let Some((is_system, expected)) = expected {
        match result.exclusive_location.unwrap() {
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
        assert_eq!(result.exclusive_location, None);
    }
}

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
async fn message_exclusive_location_on_save(
    mut labels: Vec<Label>,
    expected: Option<(bool, &str)>,
) {
    // Setup:
    //   * create a message with some labels
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut address = test_address();
    let mut tether = stash.connection();

    let mut conversation = Conversation::default();
    let message = tether
        .tx::<_, _, StashError>(async |tx| {
            address.save(tx).await.unwrap();

            conversation.save(tx).await.unwrap();

            for label in &mut labels {
                label.save(tx).await.unwrap();
            }

            let mut message = Message {
                local_conversation_id: conversation.local_id,
                local_address_id: address.id(),
                label_ids: labels
                    .iter()
                    .map(|l| l.remote_id.clone().unwrap())
                    .collect_vec(),
                ..Default::default()
            };

            // Action
            message.save(tx).await.unwrap();
            Ok(message)
        })
        .await
        .unwrap();

    // Validation
    if let Some((is_system, expected)) = expected {
        match message.exclusive_location.unwrap() {
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
        assert_eq!(message.exclusive_location, None);
    }
}

async fn test_create_message_dependencies_core(tether: &mut Tether) {
    create_address(tether).await;
}

async fn test_create_message_dependencies(tether: &mut Tether) -> LocalConversationId {
    create_labels(tether).await;
    let mut conversation: Conversation = test_conversation(
        vec![ApiConversationLabel {
            id: MY_LABEL_ID1.clone(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
            context_expiration_time: 0,
            context_snooze_time: 0,
        }],
        vec![],
    )
    .into();

    tether
        .tx::<_, _, StashError>(async |tx| conversation.save(tx).await)
        .await
        .unwrap();

    conversation.id()
}

fn test_message_metadata(
    label_ids: impl IntoIterator<Item = LabelId>,
    attachments: impl IntoIterator<Item = ApiAttachmentMetadata>,
) -> ApiMessageMetadata {
    ApiMessageMetadata {
        id: MY_MESSAGE_ID.clone(),
        conversation_id: MY_CONVERSATION_ID.clone(),
        order: 1,
        address_id: MY_ADDRESS_ID.clone(),
        label_ids: label_ids.into_iter().collect(),
        external_id: None,
        subject: "Hello ".to_owned(),
        sender: ApiMessageSender {
            address: "hello@world.com".to_owned(),
            name: "hello".to_owned(),
            is_proton: Default::default(),
            display_sender_image: Default::default(),
            is_simple_login: Default::default(),
            bimi_selector: None,
        },
        to_list: vec![],
        cc_list: vec![],
        bcc_list: vec![],
        reply_tos: vec![],
        flags: ApiMessageFlags::AUTO | ApiMessageFlags::PHISHING_AUTO,
        time: 100,
        size: 1024,
        unread: Default::default(),
        is_replied: true,
        is_replied_all: Default::default(),
        is_forwarded: true,
        expiration_time: 10000,
        num_attachments: 24,
        attachments_metadata: attachments.into_iter().collect(),
        snooze_time: 5000,
    }
}

fn test_message_with_metadata(
    label_ids: Vec<LabelId>,
    attachments: Vec<ApiAttachmentMetadata>,
) -> ApiMessage {
    ApiMessage {
        body: ApiMessageBody {
            attachments: vec![],
            body: "".to_owned(),
            header: "".to_owned(),
            mime_type: Default::default(),
            parsed_headers: Default::default(),
        },
        metadata: ApiMessageMetadata {
            id: MY_MESSAGE_ID.clone(),
            conversation_id: MY_CONVERSATION_ID.clone(),
            order: 1,
            address_id: MY_ADDRESS_ID.clone(),
            label_ids: label_ids.into_iter().collect(),
            external_id: None,
            subject: "Hello ".to_owned(),
            sender: ApiMessageSender {
                address: "hello@world.com".to_owned(),
                name: "hello".to_owned(),
                is_proton: Default::default(),
                display_sender_image: Default::default(),
                is_simple_login: Default::default(),
                bimi_selector: None,
            },
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            reply_tos: vec![],
            flags: ApiMessageFlags::AUTO | ApiMessageFlags::PHISHING_AUTO,
            time: 100,
            size: 1024,
            unread: Default::default(),
            is_replied: true,
            is_replied_all: Default::default(),
            is_forwarded: true,
            expiration_time: 10000,
            num_attachments: 24,
            attachments_metadata: attachments.into_iter().collect(),
            snooze_time: 5000,
        },
    }
}

#[tokio::test]
async fn watch_messages_in_label() {
    // Label conversation with a label that was never assigned to the conversation.
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut conn = stash.connection();
    let mut state = new_test_label_db_state();
    prepare_db_state_core(&mut conn, &mut state.addresses).await;
    let (state, state_map) = prepare_and_patch_db_state(&mut conn, state.clone()).await;

    let local_msg_id1 = *state_map
        .messages
        .get(state.messages[0].remote_id.as_ref().unwrap())
        .unwrap();

    let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();

    conn.tx::<_, _, StashError>(async |tx| {
        Message::apply_label(local_label_id1, std::iter::once(local_msg_id1), tx)
            .await
            .expect("failed to label");
        Ok(())
    })
    .await
    .unwrap();

    let handle = Message::watch(&stash).unwrap();
    let watch_result = &handle.receiver;

    tokio::spawn(async move {
        //bypass model to only execute exactly 2 queries.
        conn.tx::<_, _, StashError>(async |tx| {
            tx.execute(
                "UPDATE messages SET unread=1 WHERE local_id=?",
                params![local_msg_id1],
            )
            .await
            .unwrap();
            tx.execute(
                "UPDATE labels SET color='OxFFFFFF' WHERE local_id=?",
                params![local_label_id1],
            )
            .await
            .unwrap();
            Ok(())
        })
        .await
        .unwrap();
    });

    watch_result.recv_async().await.unwrap();
}

pub(super) async fn resolve_local_ids(tether: &Tether, message: &mut Message) {
    if message.local_conversation_id.is_none() {
        let conversation = Conversation::find_by_remote_id(
            message.remote_conversation_id.clone().unwrap(),
            tether,
        )
        .await
        .unwrap()
        .unwrap();
        message.local_conversation_id = conversation.local_id;
    }
}

#[tokio::test]
async fn test_deleting_address_will_trigger_message_deletion() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let mut tether = stash.connection();
    let address = create_address(&mut tether).await;
    let mut conv = conversation!(remote_id: conv_id!("my_conv"));
    let id = tether
        .tx::<_, _, StashError>(async |tx| {
            conv.save(tx).await?;
            let mut msg = message!(
                remote_id: msg_id!("my_msg"),
                local_conversation_id: conv.local_id,
                remote_conversation_id: conv.remote_id.clone(),
                local_address_id: address.id(),
                remote_address_id: address.remote_id.clone().unwrap()
            );
            msg.save(tx).await?;

            Ok(msg.id())
        })
        .await
        .unwrap();
    let db_message = Message::load(id, &tether)
        .await
        .expect("failed to get message");
    assert!(db_message.is_some());
    let addresses = Address::all(&tether).await.unwrap();
    assert_eq!(addresses.len(), 1);
    tether
        .tx::<_, _, StashError>(async |tx| Ok(Address::delete_all(tx).await?))
        .await
        .unwrap();
    let addresses = Address::all(&tether).await.unwrap();
    assert_eq!(addresses.len(), 0);
    let db_message = Message::load(id, &tether)
        .await
        .expect("failed to get message");
    assert!(db_message.is_none());
}
