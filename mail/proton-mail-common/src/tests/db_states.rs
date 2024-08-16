use crate::datatypes::{
    attachment, AttachmentMetadata, Disposition, LabelType, MessageAddress, MessageAddresses,
    SystemLabelId,
};
use crate::models::{Conversation, ConversationLabel, Label, Message};
use crate::tests::common::{
    test_address, test_label1, test_label2, MY_ADDRESS_ID, MY_LABEL_ID1, MY_LABEL_ID2,
};
use crate::tests::utils::TestDBState;
use lazy_static::lazy_static;
use proton_core_common::datatypes::{LabelId, RemoteId};

lazy_static! {
    pub(super) static ref DELETE_DB_CONV1: RemoteId = RemoteId::from("MyConvId1");
    pub(super) static ref DELETE_DB_CONV2: RemoteId = RemoteId::from("MyConvId2");
}

pub fn new_message_id(num: usize) -> RemoteId {
    RemoteId::from(format!("RemoteId{num}"))
}
pub fn new_test_delete_db_state() -> TestDBState {
    // Conversation 1 has 3 messages, split between 2 labels, 1 is unread  + 1 Attachment(s)
    // Conversation 2 has 2 message in one label, 1 is unread + 0 Attachment(s)
    let conv_id1 = DELETE_DB_CONV1.clone();
    let conv_id2 = DELETE_DB_CONV2.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![
            test_label1(),
            test_label2(),
            Label {
                local_id: None,
                remote_id: Some(LabelId::all_mail().clone()),
                local_parent_id: None,
                remote_parent_id: None,
                name: "All Mail".to_owned(),
                path: None,
                color: String::new().into(),
                label_type: LabelType::System,
                notify: Default::default(),
                display: Default::default(),
                sticky: Default::default(),
                expanded: Default::default(),
                initialized_conv: false,
                display_order: 5,
                initialized_msg: false,
                total_conv: 0,
                total_msg: 0,
                unread_conv: 0,
                unread_msg: 0,
                row_id: None,
                stash: None,
            },
        ],
        conversations: vec![
            Conversation {
                remote_id: Some(conv_id1.clone()),
                labels: vec![
                    ConversationLabel {
                        local_id: None,
                        local_conversation_id: None,
                        local_label_id: None,
                        remote_label_id: Some(MY_LABEL_ID1.clone().into()),
                        context_num_unread: 0,
                        context_num_messages: 0,
                        context_time: 0,
                        context_size: 0,
                        context_num_attachments: 0,
                        context_expiration_time: 0,
                        context_snooze_time: 0,
                        row_id: None,
                        stash: None,
                    },
                    ConversationLabel {
                        local_id: None,
                        local_conversation_id: None,
                        local_label_id: None,
                        remote_label_id: Some(MY_LABEL_ID2.clone().into()),
                        context_num_unread: 0,
                        context_num_messages: 0,
                        context_time: 0,
                        context_size: 0,
                        context_num_attachments: 0,
                        context_expiration_time: 0,
                        context_snooze_time: 0,
                        row_id: None,
                        stash: None,
                    },
                ],
                ..Default::default()
            },
            Conversation {
                remote_id: Some(conv_id2.clone()),
                labels: vec![ConversationLabel {
                    local_id: None,
                    local_conversation_id: None,
                    local_label_id: None,
                    remote_label_id: Some(MY_LABEL_ID2.clone().into()),
                    context_num_unread: 0,
                    context_num_messages: 0,
                    context_time: 0,
                    context_size: 0,
                    context_num_attachments: 0,
                    context_expiration_time: 0,
                    context_snooze_time: 0,
                    row_id: None,
                    stash: None,
                }],
                ..Default::default()
            },
        ],
        messages: vec![
            // Conv1 Message 1
            Message {
                remote_id: Some(new_message_id(0)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 0,
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                subject: "Message subject".to_owned(),
                sender: MessageAddress {
                    address: "bar@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "foo@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 100,
                size: 512,
                num_attachments: 1,
                attachments_metadata: vec![AttachmentMetadata {
                    local_id: None,
                    remote_id: Some(RemoteId::from("MyAttachId")),
                    size: 1024,
                    filename: "text.text".to_owned(),
                    mime_type: attachment::MimeType::text_plain(),
                    disposition: Disposition::Inline,
                }],
                ..Default::default()
            },
            // Conv1 Message 2
            Message {
                remote_id: Some(new_message_id(1)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 1,
                label_ids: vec![MY_LABEL_ID2.clone().into()],
                subject: "FW: Message subject".to_owned(),
                sender: MessageAddress {
                    address: "foo@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "omega@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 200,
                size: 100,
                is_forwarded: true,
                snooze_time: 1000,
                ..Default::default()
            },
            // Conv1 Message 3
            Message {
                remote_id: Some(new_message_id(3)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 1,
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                subject: "RE: FW: Message subject".to_owned(),
                sender: MessageAddress {
                    address: "omega@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "foo@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 400,
                size: 300,
                unread: true,
                snooze_time: 2000,
                ..Default::default()
            },
            // Conv1 Message 4
            Message {
                remote_id: Some(new_message_id(10)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                subject: "FW: Message subject".to_owned(),
                sender: MessageAddress {
                    address: "bar@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "foo@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 450,
                size: 100,
                snooze_time: 1500,
                ..Default::default()
            },
            // Conv2 Message 1
            Message {
                remote_id: Some(new_message_id(4)),
                remote_conversation_id: Some(conv_id2.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 1,
                label_ids: vec![MY_LABEL_ID2.clone().into()],
                subject: "Test".to_owned(),
                sender: MessageAddress {
                    address: "sponge.bob@square.pants".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "patrick@start.fish".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                cc_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "venture@bros.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 300,
                size: 300,
                ..Default::default()
            },
            // Conv2 Message 2
            Message {
                remote_id: Some(new_message_id(5)),
                remote_conversation_id: Some(conv_id2.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 1,
                label_ids: vec![MY_LABEL_ID2.clone().into()],
                subject: "RE: Test".to_owned(),
                sender: MessageAddress {
                    address: "venture@bros.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "sponge.bob@square.pants".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 800,
                size: 300,
                unread: true,
                ..Default::default()
            },
        ],
    }
}

pub fn new_test_unread_db_state() -> TestDBState {
    // Conversation 1 has 4 messages, split between 2 labels, All unread.
    let conv_id1 = DELETE_DB_CONV1.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1(), test_label2()],
        conversations: vec![Conversation {
            remote_id: Some(conv_id1.clone()),
            labels: vec![
                ConversationLabel {
                    local_id: None,
                    local_conversation_id: None,
                    local_label_id: None,
                    remote_label_id: Some(MY_LABEL_ID1.clone().into()),
                    context_num_unread: 0,
                    context_num_messages: 0,
                    context_time: 0,
                    context_size: 0,
                    context_num_attachments: 0,
                    context_snooze_time: 0,
                    context_expiration_time: 0,
                    row_id: None,
                    stash: None,
                },
                ConversationLabel {
                    local_id: None,
                    local_conversation_id: None,
                    local_label_id: None,
                    remote_label_id: Some(MY_LABEL_ID2.clone().into()),
                    context_num_unread: 0,
                    context_num_messages: 0,
                    context_time: 0,
                    context_size: 0,
                    context_num_attachments: 0,
                    context_expiration_time: 0,
                    context_snooze_time: 0,
                    row_id: None,
                    stash: None,
                },
            ],
            ..Default::default()
        }],
        messages: vec![
            // Conv1 Message 1
            Message {
                remote_id: Some(new_message_id(0)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                subject: "Message subject".to_owned(),
                sender: MessageAddress {
                    address: "bar@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "foo@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 100,
                size: 512,
                unread: true,
                expiration_time: 0,
                num_attachments: 1,
                attachments_metadata: vec![AttachmentMetadata {
                    local_id: None,
                    remote_id: Some(RemoteId::from("MyAttachId")),
                    size: 1024,
                    filename: "text.text".to_owned(),
                    mime_type: attachment::MimeType::text_plain(),
                    disposition: Disposition::Inline,
                }],
                ..Default::default()
            },
            // Conv1 Message 2
            Message {
                remote_id: Some(new_message_id(1)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 1,
                label_ids: vec![MY_LABEL_ID2.clone().into()],
                subject: "FW: Message subject".to_owned(),
                sender: MessageAddress {
                    address: "foo@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "omega@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 200,
                size: 100,
                unread: true,
                is_forwarded: true,
                ..Default::default()
            },
            // Conv1 Message 3
            Message {
                remote_id: Some(new_message_id(3)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 1,
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                subject: "RE: FW: Message subject".to_owned(),
                sender: MessageAddress {
                    address: "omega@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "foo@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 400,
                size: 300,
                unread: true,
                ..Default::default()
            },
            // Conv1 Message 4
            Message {
                remote_id: Some(new_message_id(10)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                label_ids: vec![MY_LABEL_ID1.clone().into()],
                subject: "FW: Message subject".to_owned(),
                sender: MessageAddress {
                    address: "bar@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "foo@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 450,
                size: 100,
                unread: true,
                ..Default::default()
            },
        ],
    }
}

pub fn new_test_label_db_state() -> TestDBState {
    let conv_id1 = DELETE_DB_CONV1.clone();
    TestDBState {
        addresses: vec![test_address()],
        labels: vec![test_label1()],
        conversations: vec![Conversation {
            remote_id: Some(conv_id1.clone()),
            ..Default::default()
        }],
        messages: vec![
            // Conv1 Message 1
            Message {
                remote_id: Some(new_message_id(0)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                subject: "Message subject".to_owned(),
                sender: MessageAddress {
                    address: "bar@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "foo@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 100,
                size: 512,
                expiration_time: 100,
                num_attachments: 1,
                attachments_metadata: vec![AttachmentMetadata {
                    local_id: None,
                    remote_id: Some(RemoteId::from("MyAttachId")),
                    size: 1024,
                    filename: "text.text".to_owned(),
                    mime_type: attachment::MimeType::text_plain(),
                    disposition: Disposition::Inline,
                }],
                snooze_time: 1000,
                ..Default::default()
            },
            // Conv1 Message 2
            Message {
                remote_id: Some(new_message_id(1)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 1,
                subject: "FW: Message subject".to_owned(),
                sender: MessageAddress {
                    address: "foo@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "omega@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 200,
                size: 100,
                is_forwarded: true,
                expiration_time: 900,
                num_attachments: 0,
                snooze_time: 2000,
                ..Default::default()
            },
            // Conv1 Message 3
            Message {
                remote_id: Some(new_message_id(3)),
                remote_conversation_id: Some(conv_id1.clone()),
                address_id: MY_ADDRESS_ID.clone().into(),
                display_order: 1,
                label_ids: vec![],
                subject: "RE: FW: Message subject".to_owned(),
                sender: MessageAddress {
                    address: "omega@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                },
                to_list: MessageAddresses {
                    value: vec![MessageAddress {
                        address: "foo@bar.com".to_owned(),
                        name: String::new(),
                        is_proton: Default::default(),
                        display_sender_image: Default::default(),
                        is_simple_login: Default::default(),
                        bimi_selector: None,
                    }],
                },
                time: 400,
                size: 300,
                unread: true,
                expiration_time: 400,
                num_attachments: 0,
                snooze_time: 1500,
                ..Default::default()
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
                local_id: None,
                local_conversation_id: None,
                local_label_id: None,
                remote_label_id: Some(MY_LABEL_ID2.clone().into()),
                context_num_unread: 0,
                context_num_messages: 0,
                context_time: 0,
                context_size: 0,
                context_num_attachments: 0,
                context_expiration_time: 0,
                context_snooze_time: 0,
                row_id: None,
                stash: None,
            }],
            ..Default::default()
        }],
        messages: vec![Message {
            remote_id: Some(new_message_id(0)),
            remote_conversation_id: Some(conv_id1.clone()),
            address_id: MY_ADDRESS_ID.clone().into(),
            display_order: 0,
            label_ids: vec![MY_LABEL_ID2.clone().into()],
            subject: "Message subject".to_owned(),
            sender: MessageAddress {
                address: "bar@bar.com".to_owned(),
                name: String::new(),
                is_proton: Default::default(),
                display_sender_image: Default::default(),
                is_simple_login: Default::default(),
                bimi_selector: None,
            },
            to_list: MessageAddresses {
                value: vec![MessageAddress {
                    address: "foo@bar.com".to_owned(),
                    name: String::new(),
                    is_proton: Default::default(),
                    display_sender_image: Default::default(),
                    is_simple_login: Default::default(),
                    bimi_selector: None,
                }],
            },
            time: 100,
            size: 512,
            expiration_time: 100,
            num_attachments: 1,
            attachments_metadata: vec![AttachmentMetadata {
                local_id: None,
                remote_id: Some(RemoteId::from("MyAttachId")),
                size: 1024,
                filename: "text.text".to_owned(),
                mime_type: attachment::MimeType::text_plain(),
                disposition: Disposition::Inline,
            }],
            snooze_time: 1000,
            ..Default::default()
        }],
    }
}
