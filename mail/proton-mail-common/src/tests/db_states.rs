use std::sync::LazyLock;

use crate::datatypes::{
    attachment, AttachmentMetadata, Disposition, MessageAddresses, SystemLabelId as _,
};
use crate::models::{Conversation, ConversationLabel, Label, Message};
use crate::tests::common::{
    test_address, test_label1, test_label2, MY_ADDRESS_ID, MY_LABEL_ID1, MY_LABEL_ID2,
};
use crate::tests::utils::TestDBState;
use lazy_static::lazy_static;
use proton_core_common::datatypes::{LabelId, RemoteId};

// ------- TEST DATA -------

lazy_static! {
    pub(super) static ref DELETE_DB_CONV1: RemoteId = RemoteId::from("MyConvId1");
    pub(super) static ref DELETE_DB_CONV2: RemoteId = RemoteId::from("MyConvId2");
}

static TEXT_ATTACHMENT: LazyLock<AttachmentMetadata> = LazyLock::new(|| AttachmentMetadata {
    local_id: None,
    remote_id: Some(RemoteId::from("MyAttachId")),
    size: 1024,
    filename: "text.text".to_owned(),
    mime_type: attachment::MimeType::text_plain(),
    disposition: Disposition::Inline,
});

static BASE_CONV1_MESSAGE: LazyLock<Message> = LazyLock::new(|| Message {
    remote_conversation_id: Some(DELETE_DB_CONV1.clone()),
    remote_address_id: MY_ADDRESS_ID.clone().into(),
    local_address_id: 1.into(),
    ..Default::default()
});

static CONV1_MSG1: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(0)),
    subject: "Message subject".to_owned(),
    sender: "bar@bar.com".into(),
    to_list: MessageAddresses {
        value: vec!["foo@bar.com".into()],
    },
    time: 100,
    size: 512,
    snooze_time: 1000,
    ..BASE_CONV1_MESSAGE.to_owned()
});

/// From bar@bar.com to foo@bar.com
/// One text attachment
static CONV1_MSG2: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(1)),
    display_order: 1,
    subject: "FW: Message subject".to_owned(),
    sender: "foo@bar.com".into(),
    to_list: MessageAddresses {
        value: vec!["omega@bar.com".into()],
    },
    time: 200,
    size: 100,
    is_forwarded: true,
    snooze_time: 2000,
    ..BASE_CONV1_MESSAGE.to_owned()
});

static CONV1_MSG3: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(3)),
    display_order: 1,
    subject: "RE: FW: Message subject".to_owned(),
    sender: "omega@bar.com".into(),
    to_list: MessageAddresses {
        value: vec!["foo@bar.com".into()],
    },
    time: 400,
    size: 300,
    unread: true,
    snooze_time: 1500,
    ..BASE_CONV1_MESSAGE.to_owned()
});

static CONV1_MSG4: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(10)),
    subject: "FW: Message subject".to_owned(),
    sender: "bar@bar.com".into(),
    to_list: MessageAddresses {
        value: vec!["foo@bar.com".into()],
    },
    time: 450,
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
    label_ids: vec![MY_LABEL_ID2.clone().into()],
    subject: "Test".to_owned(),
    sender: "sponge.bob@square.pants".into(),
    to_list: MessageAddresses {
        value: vec!["patrick@start.fish".into()],
    },
    cc_list: MessageAddresses {
        value: vec!["venture@bros.com".into()],
    },
    time: 300,
    size: 300,
    ..BASE_CONV2_MESSAGE.to_owned()
});

static CONV2_MSG2: LazyLock<Message> = LazyLock::new(|| Message {
    remote_id: Some(new_message_id(5)),
    display_order: 1,
    label_ids: vec![MY_LABEL_ID2.clone().into()],
    subject: "RE: Test".to_owned(),
    sender: "venture@bros.com".into(),
    to_list: MessageAddresses {
        value: vec!["sponge.bob@square.pants".into()],
    },
    time: 800,
    size: 300,
    unread: true,
    ..BASE_CONV2_MESSAGE.to_owned()
});

static CONV_LABEL1: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
    remote_label_id: Some(MY_LABEL_ID1.clone().into()),
    ..Default::default()
});

static CONV_LABEL2: LazyLock<ConversationLabel> = LazyLock::new(|| ConversationLabel {
    remote_label_id: Some(MY_LABEL_ID2.clone().into()),
    ..Default::default()
});

// ------- init fns -------

pub fn new_test_label_db_state() -> TestDBState {
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1()],
        conversations: vec![Conversation {
            remote_id: Some(DELETE_DB_CONV1.clone()),
            labels: vec![],
            ..Default::default()
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

pub fn new_message_id(num: usize) -> RemoteId {
    RemoteId::from(format!("RemoteId{num}"))
}

pub fn new_test_delete_db_state() -> TestDBState {
    // Conversation 1 has 4 messages, split between 2 labels, 1 is unread  + 1 Attachment(s)
    // Conversation 2 has 2 message in one label, 1 is unread + 0 Attachment(s)
    let conv_id1 = DELETE_DB_CONV1.clone();
    let conv_id2 = DELETE_DB_CONV2.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![
            test_label1(),
            test_label2(),
            Label {
                remote_id: Some(LabelId::all_mail().clone()),
                name: "All Mail".to_owned(),
                ..Default::default()
            },
        ],
        conversations: vec![
            Conversation {
                remote_id: Some(conv_id1.clone()),
                labels: vec![CONV_LABEL1.to_owned(), CONV_LABEL2.to_owned()],
                ..Default::default()
            },
            Conversation {
                remote_id: Some(conv_id2.clone()),
                labels: vec![CONV_LABEL2.to_owned()],
                ..Default::default()
            },
        ],
        messages: vec![
            Message {
                attachments_metadata: vec![TEXT_ATTACHMENT.to_owned()],
                num_attachments: 1,
                label_ids: vec![MY_LABEL_ID2.clone().into()],
                ..CONV1_MSG1.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                ..CONV1_MSG2.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                ..CONV1_MSG3.to_owned()
            },
            Message {
                unread: true,
                label_ids: vec![MY_LABEL_ID2.clone().into()],
                ..CONV1_MSG4.to_owned()
            },
            // conversation 2
            CONV2_MSG1.to_owned(),
            CONV2_MSG2.to_owned(),
        ],
    }
}

pub fn new_test_unread_db_state() -> TestDBState {
    // Conversation 1 has 4 messages, All unread.
    // 3 are in label1 and 1 in label2
    let conv_id1 = DELETE_DB_CONV1.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1(), test_label2()],
        conversations: vec![Conversation {
            remote_id: Some(conv_id1.clone()),
            labels: vec![
                ConversationLabel {
                    remote_label_id: Some(MY_LABEL_ID1.clone().into()),
                    ..Default::default()
                },
                ConversationLabel {
                    remote_label_id: Some(MY_LABEL_ID2.clone().into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        messages: vec![
            Message {
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                unread: true,
                ..CONV1_MSG1.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID2.clone().into()],
                unread: true,
                ..CONV1_MSG2.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                unread: true,
                ..CONV1_MSG3.to_owned()
            },
            Message {
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                unread: true,
                ..CONV1_MSG4.to_owned()
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
                remote_label_id: Some(MY_LABEL_ID2.clone().into()),
                ..Default::default()
            }],
            ..Default::default()
        }],
        messages: vec![CONV1_MSG1.to_owned()],
    }
}
