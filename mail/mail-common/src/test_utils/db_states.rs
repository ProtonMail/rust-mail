use std::convert::Into;
use std::sync::LazyLock;

use crate::datatypes::{
    AttachmentMetadata, Disposition, MessageFlags, MessageRecipients, SystemLabelId as _,
    attachment,
};
use crate::models::{AttachmentType, Conversation, ConversationLabel, Message};
use crate::test_utils::search::{
    MY_ADDRESS_ID, MY_LABEL_ID1, MY_LABEL_ID2, test_label1, test_label2,
};
use crate::test_utils::utils::{TestDBState, test_address};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LabelType, UnixTimestamp};
use proton_core_common::models::Label;
use proton_mail_api::services::proton::common::{AttachmentId, ConversationId, MessageId};

// ------- TEST DATA -------

pub(super) static DELETE_DB_CONV1: LazyLock<ConversationId> =
    LazyLock::new(|| ConversationId::from("MyConvId1"));
pub(super) static DELETE_DB_CONV2: LazyLock<ConversationId> =
    LazyLock::new(|| ConversationId::from("MyConvId2"));

static TEXT_ATTACHMENT: LazyLock<AttachmentMetadata> = LazyLock::new(|| AttachmentMetadata {
    local_id: None,
    attachment_type: AttachmentType::Remote(Some(AttachmentId::from("MyAttachId"))),
    size: 1024,
    filename: "text.text".to_owned(),
    mime_type: attachment::MimeType::text_plain(),
    disposition: Disposition::Inline,
});

static BASE_CONV1_MESSAGE: LazyLock<Message> = LazyLock::new(|| Message {
    remote_conversation_id: Some(DELETE_DB_CONV1.clone()),
    remote_address_id: MY_ADDRESS_ID.clone(),
    local_address_id: 1.into(),
    ..Message::test_default()
});

static CONV1_MSG1: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(0)),
    subject: "Message subject".to_owned(),
    sender: "bar@bar.com".into(),
    to_list: MessageRecipients {
        value: vec!["foo@bar.com".into()],
    },
    time: 100.into(),
    size: 512,
    snooze_time: 1000.into(),
    ..BASE_CONV1_MESSAGE.to_owned()
});

/// From bar@bar.com to foo@bar.com
/// One text attachment
static CONV1_MSG2: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(1)),
    display_order: 1,
    subject: "FW: Message subject".to_owned(),
    sender: "foo@bar.com".into(),
    to_list: MessageRecipients {
        value: vec!["omega@bar.com".into()],
    },
    time: 200.into(),
    size: 100,
    is_forwarded: true,
    snooze_time: 2000.into(),
    ..BASE_CONV1_MESSAGE.to_owned()
});

static CONV1_MSG3: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(3)),
    display_order: 1,
    subject: "RE: FW: Message subject".to_owned(),
    sender: "omega@bar.com".into(),
    to_list: MessageRecipients {
        value: vec!["foo@bar.com".into()],
    },
    time: 400.into(),
    size: 300,
    unread: true,
    snooze_time: 1500.into(),
    ..BASE_CONV1_MESSAGE.to_owned()
});

static CONV1_MSG4: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(10)),
    subject: "FW: Message subject".to_owned(),
    sender: "bar@bar.com".into(),
    to_list: MessageRecipients {
        value: vec!["foo@bar.com".into()],
    },
    time: 450.into(),
    size: 100,
    unread: true,
    ..BASE_CONV1_MESSAGE.to_owned()
});

static BASE_CONV2_MESSAGE: LazyLock<Message> = LazyLock::new(|| Message {
    remote_conversation_id: Some(DELETE_DB_CONV2.clone()),
    ..BASE_CONV1_MESSAGE.to_owned()
});

static CONV2_MSG1: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(4)),
    display_order: 1,
    label_ids: vec![MY_LABEL_ID2.clone()],
    subject: "Test".to_owned(),
    sender: "sponge.bob@square.pants".into(),
    to_list: MessageRecipients {
        value: vec!["patrick@start.fish".into()],
    },
    cc_list: MessageRecipients {
        value: vec!["venture@bros.com".into()],
    },
    time: 300.into(),
    size: 300,
    ..BASE_CONV2_MESSAGE.to_owned()
});

static CONV2_MSG2: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(5)),
    display_order: 1,
    label_ids: vec![MY_LABEL_ID2.clone()],
    subject: "RE: Test".to_owned(),
    sender: "venture@bros.com".into(),
    to_list: MessageRecipients {
        value: vec!["sponge.bob@square.pants".into()],
    },
    time: 800.into(),
    size: 300,
    unread: true,
    ..BASE_CONV2_MESSAGE.to_owned()
});

static CONV_LABEL1: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
    remote_label_id: Some(MY_LABEL_ID1.clone()),
    ..ConversationLabel::test_default()
});

static CONV_LABEL2: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
    remote_label_id: Some(MY_LABEL_ID2.clone()),
    ..ConversationLabel::test_default()
});

// ------- init fns -------

pub fn new_test_label_db_state() -> TestDBState {
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1()],
        conversations: vec![Conversation {
            remote_id: Some(DELETE_DB_CONV1.clone()),
            labels: vec![],
            expiration_time: UnixTimestamp::new(0),
            ..Conversation::test_default()
        }],
        messages: vec![
            Message {
                attachments_metadata: vec![TEXT_ATTACHMENT.to_owned()],
                num_attachments: 1,
                ..CONV1_MSG1.to_owned()
            },
            CONV1_MSG2.to_owned(),
            CONV1_MSG3.to_owned(),
        ],
    }
}

pub fn new_test_label_expiration_db_state() -> TestDBState {
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1()],
        conversations: vec![Conversation {
            remote_id: Some(DELETE_DB_CONV1.clone()),
            labels: vec![],
            expiration_time: UnixTimestamp::new(0),
            ..Conversation::test_default()
        }],
        messages: vec![
            Message {
                expiration_time: UnixTimestamp::new(1000),
                ..CONV1_MSG1.to_owned()
            },
            Message {
                expiration_time: UnixTimestamp::new(700),
                ..CONV1_MSG2.to_owned()
            },
            Message {
                expiration_time: UnixTimestamp::new(200),
                ..CONV1_MSG3.to_owned()
            },
        ],
    }
}

#[must_use]
pub fn new_message_id(num: usize) -> MessageId {
    MessageId::from(format!("RemoteId{num}"))
}

pub fn new_test_delete_db_state() -> TestDBState {
    // Conversation 1 has 4 messages, split between 2 labels, 1 is unread  + 1 Attachment(s)
    // Conversation 2 has 2 message in one label, 1 is unread + 0 Attachment(s)
    let conv_id1 = DELETE_DB_CONV1.clone();
    let conv_id2 = DELETE_DB_CONV2.clone();
    let all_mail = Label {
        remote_id: Some(LabelId::all_mail().clone()),
        name: "All Mail".to_owned(),
        ..Label::test_default()
    };
    let all_mail_conv_label = ConversationLabel {
        remote_label_id: Some(all_mail.remote_id.clone().unwrap()),
        ..ConversationLabel::test_default()
    };
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1(), test_label2(), all_mail.clone()],
        conversations: vec![
            Conversation {
                remote_id: Some(conv_id1.clone()),
                labels: vec![
                    CONV_LABEL1.to_owned(),
                    CONV_LABEL2.to_owned(),
                    all_mail_conv_label.clone(),
                ],
                is_known: true,
                ..Conversation::test_default()
            },
            Conversation {
                remote_id: Some(conv_id2.clone()),
                labels: vec![CONV_LABEL2.to_owned(), all_mail_conv_label.clone()],
                is_known: true,
                ..Conversation::test_default()
            },
        ],
        messages: vec![
            Message {
                attachments_metadata: vec![TEXT_ATTACHMENT.to_owned()],
                num_attachments: 1,
                label_ids: vec![MY_LABEL_ID2.clone(), all_mail.remote_id.clone().unwrap()],
                ..CONV1_MSG1.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone(), all_mail.remote_id.clone().unwrap()],
                ..CONV1_MSG2.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone(), all_mail.remote_id.clone().unwrap()],
                unread: true,
                ..CONV1_MSG3.to_owned()
            },
            Message {
                unread: true,
                label_ids: vec![MY_LABEL_ID2.clone(), all_mail.remote_id.clone().unwrap()],
                ..CONV1_MSG4.to_owned()
            },
            // conversation 2
            {
                let mut msg_1 = CONV2_MSG1.to_owned();
                msg_1.label_ids.push(all_mail.remote_id.clone().unwrap());
                msg_1
            },
            {
                let mut msg_2 = CONV2_MSG2.to_owned();
                msg_2.label_ids.push(all_mail.remote_id.clone().unwrap());
                msg_2
            },
        ],
    }
}

pub fn new_test_delete_all_messages_in_conv_label_db_state() -> TestDBState {
    // Conversation 1 has 4 messages, split between 2 labels
    let conv_id1 = DELETE_DB_CONV1.clone();
    let all_mail = Label {
        remote_id: Some(LabelId::all_mail().clone()),
        name: "All Mail".to_owned(),
        ..Label::test_default()
    };
    let all_mail_conv_label = ConversationLabel {
        remote_label_id: Some(all_mail.remote_id.clone().unwrap()),
        ..ConversationLabel::test_default()
    };
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1(), test_label2(), all_mail.clone()],
        conversations: vec![Conversation {
            remote_id: Some(conv_id1.clone()),
            labels: vec![
                CONV_LABEL1.to_owned(),
                CONV_LABEL2.to_owned(),
                all_mail_conv_label.clone(),
            ],
            is_known: true,
            ..Conversation::test_default()
        }],
        messages: vec![
            Message {
                attachments_metadata: vec![TEXT_ATTACHMENT.to_owned()],
                num_attachments: 1,
                label_ids: vec![MY_LABEL_ID2.clone(), all_mail.remote_id.clone().unwrap()],
                ..CONV1_MSG1.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone(), all_mail.remote_id.clone().unwrap()],
                ..CONV1_MSG2.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone(), all_mail.remote_id.clone().unwrap()],
                ..CONV1_MSG3.to_owned()
            },
            Message {
                unread: true,
                label_ids: vec![MY_LABEL_ID2.clone(), all_mail.remote_id.clone().unwrap()],
                ..CONV1_MSG4.to_owned()
            },
        ],
    }
}

/// Conversation 1 has 4 messages, All unread.
/// 3 are in label1 and 1 in label2
pub fn new_test_unread_db_state() -> TestDBState {
    let conv_id1 = DELETE_DB_CONV1.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1(), test_label2()],
        conversations: vec![Conversation {
            remote_id: Some(conv_id1.clone()),
            labels: vec![
                ConversationLabel {
                    remote_label_id: Some(MY_LABEL_ID1.clone()),
                    ..ConversationLabel::test_default()
                },
                ConversationLabel {
                    remote_label_id: Some(MY_LABEL_ID2.clone()),
                    ..ConversationLabel::test_default()
                },
            ],
            ..Conversation::test_default()
        }],
        messages: vec![
            Message {
                label_ids: vec![MY_LABEL_ID1.clone()],
                unread: true,
                time: 100.into(),
                ..CONV1_MSG1.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID2.clone()],
                unread: true,
                time: 200.into(),
                ..CONV1_MSG2.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone()],
                unread: true,
                time: 300.into(),
                ..CONV1_MSG3.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone()],
                unread: true,
                time: 400.into(),
                ..CONV1_MSG4.to_owned()
            },
        ],
    }
}

pub fn new_test_unread_db_state_multi_conv() -> TestDBState {
    let conv_id1 = DELETE_DB_CONV1.clone();
    let conv_id2 = DELETE_DB_CONV2.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1(), test_label2()],
        conversations: vec![
            Conversation {
                remote_id: Some(conv_id1.clone()),
                labels: vec![
                    ConversationLabel {
                        remote_label_id: Some(MY_LABEL_ID1.clone()),
                        ..ConversationLabel::test_default()
                    },
                    ConversationLabel {
                        remote_label_id: Some(MY_LABEL_ID2.clone()),
                        ..ConversationLabel::test_default()
                    },
                ],
                ..Conversation::test_default()
            },
            Conversation {
                remote_id: Some(conv_id2.clone()),
                labels: vec![
                    ConversationLabel {
                        remote_label_id: Some(MY_LABEL_ID1.clone()),
                        ..ConversationLabel::test_default()
                    },
                    ConversationLabel {
                        remote_label_id: Some(MY_LABEL_ID2.clone()),
                        ..ConversationLabel::test_default()
                    },
                ],
                ..Conversation::test_default()
            },
        ],
        messages: vec![
            Message {
                label_ids: vec![MY_LABEL_ID1.clone()],
                unread: true,
                time: 100.into(),
                ..CONV1_MSG1.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID2.clone()],
                unread: true,
                time: 200.into(),
                ..CONV1_MSG2.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone()],
                unread: true,
                time: 300.into(),
                ..CONV2_MSG1.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID2.clone()],
                unread: true,
                time: 400.into(),
                ..CONV2_MSG2.to_owned()
            },
        ],
    }
}

pub fn new_test_unread_db_state_unread_label_in_folder() -> TestDBState {
    let conv_id1 = DELETE_DB_CONV1.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1(), test_label2()],
        conversations: vec![Conversation {
            remote_id: Some(conv_id1.clone()),
            labels: vec![
                ConversationLabel {
                    remote_label_id: Some(MY_LABEL_ID1.clone()),
                    ..ConversationLabel::test_default()
                },
                ConversationLabel {
                    remote_label_id: Some(MY_LABEL_ID2.clone()),
                    ..ConversationLabel::test_default()
                },
            ],
            ..Conversation::test_default()
        }],
        messages: vec![
            Message {
                label_ids: vec![MY_LABEL_ID1.clone()],
                unread: false,
                time: 100.into(),
                ..CONV1_MSG1.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID2.clone()],
                unread: false,
                time: 200.into(),
                ..CONV1_MSG2.to_owned()
            },
        ],
    }
}

/// Database state where there is one conversation which has one label applied and we
/// need to tes that if we apply another label it does populate the table with 0
/// values.
pub fn new_test_label_db_state_label_with_existing_labels() -> TestDBState {
    let conv_id1 = DELETE_DB_CONV1.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1(), test_label2()],
        conversations: vec![Conversation {
            remote_id: Some(conv_id1.clone()),
            labels: vec![ConversationLabel {
                remote_label_id: Some(MY_LABEL_ID2.clone()),
                ..ConversationLabel::test_default()
            }],
            ..Conversation::test_default()
        }],
        messages: vec![CONV1_MSG1.to_owned()],
    }
}

pub fn new_conversation_snooze_db_state() -> TestDBState {
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![
            Label {
                remote_id: Some(LabelId::inbox()),
                label_type: LabelType::System,
                ..Label::test_default()
            },
            Label {
                remote_id: Some(LabelId::sent()),
                label_type: LabelType::System,
                ..Label::test_default()
            },
            Label {
                remote_id: Some(LabelId::snoozed()),
                label_type: LabelType::System,
                ..Label::test_default()
            },
            Label {
                remote_id: Some(LabelId::all_mail()),
                label_type: LabelType::System,
                ..Label::test_default()
            },
            test_label1(),
            test_label2(),
        ],
        conversations: vec![Conversation {
            remote_id: Some(DELETE_DB_CONV1.clone()),
            labels: vec![
                ConversationLabel {
                    remote_label_id: Some(LabelId::inbox()),
                    ..ConversationLabel::test_default()
                },
                ConversationLabel {
                    remote_label_id: Some(LabelId::sent()),
                    ..ConversationLabel::test_default()
                },
                ConversationLabel {
                    remote_label_id: Some(MY_LABEL_ID1.clone()),
                    ..ConversationLabel::test_default()
                },
                ConversationLabel {
                    remote_label_id: Some(MY_LABEL_ID2.clone()),
                    ..ConversationLabel::test_default()
                },
            ],
            expiration_time: UnixTimestamp::new(0),
            ..Conversation::test_default()
        }],
        messages: vec![
            Message {
                // Received message with custom label
                label_ids: vec![LabelId::inbox(), MY_LABEL_ID1.clone()],
                flags: MessageFlags::RECEIVED,
                snooze_time: UnixTimestamp::new(0),
                ..CONV1_MSG1.to_owned()
            },
            Message {
                // Sent message
                label_ids: vec![LabelId::sent()],
                flags: MessageFlags::SENT,
                snooze_time: UnixTimestamp::new(0),
                ..CONV1_MSG2.to_owned()
            },
            Message {
                // Received message in custom folder.
                label_ids: vec![MY_LABEL_ID2.clone()],
                flags: MessageFlags::RECEIVED,
                snooze_time: UnixTimestamp::new(0),
                ..CONV1_MSG3.to_owned()
            },
        ],
    }
}
