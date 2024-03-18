use crate::conversations::test_db_states::new_test_delete_db_state;
use crate::conversations::test_utils::{
    conv_counts_as_map, message_counts_for_conversation,
    msg_counts_as_map, prepare_and_patch_db_state,
};
use crate::conversations::types::LocalConversation;
use crate::{
    new_test_connection, with_tx, LabelColor, LocalAttachmentMetadata, LocalConversationCount,
    LocalConversationLabel, LocalLabelId, MailSqliteConnectionMut,
};
use lazy_static::lazy_static;
use proton_api_mail::domain::{
    Address, AddressId, AddressSignedKeyList, AddressStatus, AddressType, AttachmentId,
    AttachmentMetadata, Conversation, ConversationCount, ConversationId, ConversationLabels,
    Disposition, Label, LabelId, LabelType, MessageAddress,
};
use proton_api_mail::proton_api_core::domain::ProtonBoolean;
use proton_api_mail::proton_api_core::exports::crypto::domain::AddressKeys;

#[test]
fn test_conversation_create_no_labels() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        create_address_and_labels(tx);
        let conv = test_conversation([], []);
        let id = tx
            .create_conversation(&conv)
            .expect("failed to create conversation");

        let local_conversation = LocalConversation::from_conversation(id, conv.clone(), None);
        let db_conversation = tx
            .get_conversation(id)
            .expect("failed to get conversation")
            .expect("should have value");
        assert_eq!(local_conversation, db_conversation);
    });
}

#[test]
fn test_conversation_create_starred() {
    let conv_label = ConversationLabels {
        id: LabelId::starred().clone(),
        context_num_unread: 0,
        context_num_messages: 0,
        context_time: 0,
        context_size: 0,
        context_num_attachments: 0,
    };
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        create_address_and_labels(tx);
        tx.create_remote_label(&test_starred_label()).unwrap();

        // Add starred label, should gain starred attribute.
        let conv = test_conversation([conv_label.clone()], []);
        let id = tx
            .create_conversation(&conv)
            .expect("failed to create conversation");

        {
            let local_conversation = LocalConversation::from_conversation(id, conv.clone(), None);
            let db_conversation = tx
                .get_conversation(id)
                .expect("failed to get conversation")
                .expect("should have value");
            assert_eq!(local_conversation, db_conversation);
            assert!(local_conversation.starred);
            assert!(db_conversation.starred);
        }
        {
            let local_conversation = LocalConversation::from_conversation_and_label(
                id,
                LabelId::starred(),
                conv.clone(),
                None,
            );
            let db_conversation = tx
                .get_conversation_with_context(
                    id,
                    tx.resolve_remote_label_id(LabelId::starred())
                        .unwrap()
                        .unwrap(),
                )
                .expect("failed to get conversation")
                .expect("should have value");
            assert_eq!(local_conversation, db_conversation);
            assert!(local_conversation.starred);
            assert!(db_conversation.starred);
        }

        // Remove starred label, should lose starred attribute.
        let conv = test_conversation([], []);
        let id = tx
            .create_conversation(&conv)
            .expect("failed to create conversation");
        {
            let local_conversation = LocalConversation::from_conversation(id, conv.clone(), None);
            let db_conversation = tx
                .get_conversation(id)
                .expect("failed to get conversation")
                .expect("should have value");
            assert_eq!(local_conversation, db_conversation);
            assert!(!local_conversation.starred);
            assert!(!db_conversation.starred);
        }
    });
}

#[test]
fn test_conversation_create_with_labels() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let local_label_ids = create_address_and_labels(tx);
        let conv = test_conversation(
            [
                ConversationLabels {
                    id: MY_LABEL_ID1.clone(),
                    context_num_unread: 1,
                    context_num_messages: 2,
                    context_time: 3,
                    context_size: 4,
                    context_num_attachments: 5,
                },
                ConversationLabels {
                    id: MY_LABEL_ID2.clone(),
                    context_num_unread: 6,
                    context_num_messages: 7,
                    context_time: 8,
                    context_size: 9,
                    context_num_attachments: 10,
                },
            ],
            [],
        );
        let id = tx
            .create_conversation(&conv)
            .expect("failed to create conversation");

        for (idx, label) in [MY_LABEL_ID1.clone(), MY_LABEL_ID2.clone()]
            .iter()
            .enumerate()
        {
            let local_conversation = LocalConversation::from_conversation_and_label(
                id,
                label,
                conv.clone(),
                Some(vec![LocalConversationLabel {
                    id: local_label_ids[0],
                    name: "MyLabel".to_string(),
                    color: LabelColor::black(),
                }]),
            );
            let db_conversation = tx
                .get_conversation_with_context(id, local_label_ids[idx])
                .expect("failed to get conversation")
                .expect("should have value");
            assert_eq!(
                local_conversation, db_conversation,
                "conversation with context (LabelId={label}) do not match"
            );
        }
    });
}

#[test]
fn test_conversation_create_with_attachment() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        create_address_and_labels(tx);
        let conv = test_conversation(
            [],
            [AttachmentMetadata {
                id: MY_ATTACHMENT_ID.clone(),
                size: 4098,
                name: "My Attachment.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                disposition: Disposition::Attachment,
            }],
        );
        let id = tx
            .create_conversation(&conv)
            .expect("failed to create conversation");

        let attachments = tx
            .get_conversation_attachments(id)
            .expect("failed to get attachments")
            .expect("must have value");
        assert_eq!(attachments.len(), 1);
        let converted_attachment = LocalAttachmentMetadata::from_attachment_metadata(
            attachments[0].id,
            conv.attachments_metadata[0].clone(),
        );
        assert_eq!(attachments[0], converted_attachment);

        let db_conversation = tx.get_conversation(id).unwrap().unwrap();
        assert_eq!(
            db_conversation.attachments.unwrap()[0],
            converted_attachment
        );
    });
}

#[test]
fn test_conversation_create_with_attachment_and_label() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let local_labels = create_address_and_labels(tx);
        let conv = test_conversation(
            [ConversationLabels {
                id: MY_LABEL_ID1.clone(),
                context_num_unread: 1,
                context_num_messages: 2,
                context_time: 3,
                context_size: 4,
                context_num_attachments: 5,
            }],
            [AttachmentMetadata {
                id: MY_ATTACHMENT_ID.clone(),
                size: 4098,
                name: "My Attachment.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                disposition: Disposition::Attachment,
            }],
        );
        let id = tx
            .create_conversation(&conv)
            .expect("failed to create conversation");

        let attachments = tx
            .get_conversation_attachments(id)
            .expect("failed to get attachments")
            .expect("must have value");
        assert_eq!(attachments.len(), 1);
        let converted_attachment = LocalAttachmentMetadata::from_attachment_metadata(
            attachments[0].id,
            conv.attachments_metadata[0].clone(),
        );
        assert_eq!(attachments[0], converted_attachment);

        let db_conversation = tx
            .get_conversation_with_context(id, local_labels[0])
            .unwrap()
            .unwrap();
        assert_eq!(
            db_conversation.attachments.unwrap()[0],
            converted_attachment
        );
    });
}

#[test]
fn test_conversation_update() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let local_label_ids = create_address_and_labels(tx);
        let conv = test_conversation(
            [ConversationLabels {
                id: MY_LABEL_ID2.clone(),
                context_num_unread: 6,
                context_num_messages: 7,
                context_time: 8,
                context_size: 9,
                context_num_attachments: 10,
            }],
            [AttachmentMetadata {
                id: AttachmentId::from("ATTACHMENT2"),
                size: 224515,
                name: "Attachment.json".to_string(),
                mime_type: "application/json".to_string(),
                disposition: Disposition::Attachment,
            }],
        );

        let conv_update = test_conversation(
            [ConversationLabels {
                id: MY_LABEL_ID1.clone(),
                context_num_unread: 1,
                context_num_messages: 2,
                context_time: 3,
                context_size: 4,
                context_num_attachments: 5,
            }],
            [AttachmentMetadata {
                id: MY_ATTACHMENT_ID.clone(),
                size: 4098,
                name: "My Attachment.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                disposition: Disposition::Attachment,
            }],
        );
        let id = tx
            .create_conversation(&conv)
            .expect("failed to create conversation");

        tx.update_conversation(&conv_update)
            .expect("failed to update conversation");

        let mut local_conversation = LocalConversation::from_conversation_and_label(
            id,
            &MY_LABEL_ID1,
            conv_update.clone(),
            Some(vec![LocalConversationLabel {
                id: local_label_ids[0],
                name: "MyLabel".to_string(),
                color: LabelColor::black(),
            }]),
        );

        let attachments = tx
            .get_conversation_attachments(id)
            .expect("failed to get attachments")
            .expect("must have value");
        assert_eq!(attachments.len(), 1);
        let converted_attachment = LocalAttachmentMetadata::from_attachment_metadata(
            attachments[0].id,
            conv_update.attachments_metadata[0].clone(),
        );

        local_conversation.attachments = Some(vec![converted_attachment]);

        let db_conversation = tx
            .get_conversation_with_context(id, local_label_ids[0])
            .expect("failed to get conversation")
            .expect("should have value");
        assert_eq!(local_conversation, db_conversation,);
    });
}

#[test]
fn test_conversation_delete() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let state = new_test_delete_db_state();
        let (state, state_map) = prepare_and_patch_db_state(tx, state);

        // Deleting a conversation must
        // * Update conversation counters
        // * Update message counters

        let local_conv_id = *state_map
            .conversations
            .get(&state.conversations[0].id)
            .unwrap();
        let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
        let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
        tx.mark_conversation_as_deleted(local_conv_id)
            .expect("failed to mark as deleted");

        let db_conversation = tx
            .get_conversation(local_conv_id)
            .expect("failed to get conversation");
        assert!(db_conversation.is_none());

        // Check conversation counts
        {
            let conv_counts = conv_counts_as_map(tx);
            // Check conversation label1 values
            {
                let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                let label_counts = conv_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, start_label_counts.unread - 1,);
                assert_eq!(label_counts.total, start_label_counts.total - 1,);
            }
            // Check conversation label2 values
            {
                let start_label_counts = state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
                let label_counts = conv_counts.get(&local_label_id2).unwrap();
                assert_eq!(label_counts.unread, start_label_counts.unread,);
                assert_eq!(label_counts.total, start_label_counts.total - 1);
            }
        }

        // Check message counts
        {
            let message_counts = msg_counts_as_map(tx);

            // Check label1
            {
                let (unread, total) = message_counts_for_conversation(
                    &state.messages,
                    &state.conversations[0].id,
                    &MY_LABEL_ID1,
                );
                let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
                let label_counts = message_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, start_label_counts.unread - unread);
                assert_eq!(label_counts.total, start_label_counts.total - total);
            }
            // Check label2
            {
                let (unread, total) = message_counts_for_conversation(
                    &state.messages,
                    &state.conversations[0].id,
                    &MY_LABEL_ID2,
                );
                let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
                let label_counts = message_counts.get(&local_label_id2).unwrap();
                assert_eq!(label_counts.unread, start_label_counts.unread - unread);
                assert_eq!(label_counts.total, start_label_counts.total - total);
            }
        }

        // Deleting conv2 should reset all counters to 0.
        let local_conv_id = *state_map
            .conversations
            .get(&state.conversations[1].id)
            .unwrap();
        tx.mark_conversation_as_deleted(local_conv_id)
            .expect("failed to mark conv as deleted");

        for count in tx.get_message_counts().unwrap() {
            assert_eq!(
                count.total, 0,
                "Label {:?} does not have 0 total count",
                count.id
            );
            assert_eq!(
                count.unread, 0,
                "Label {:?} does not have 0 unread count",
                count.id
            );
        }

        for count in tx.get_conversation_counts().unwrap() {
            assert_eq!(
                count.total, 0,
                "Label {:?} does not have 0 total count",
                count.id
            );
            assert_eq!(
                count.unread, 0,
                "Label {:?} does not have 0 unread count",
                count.id
            );
        }
    });
}

#[test]
fn test_conversation_counts() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let labels = create_address_and_labels(tx);
        let counts = [
            ConversationCount {
                label_id: MY_LABEL_ID1.clone(),
                total: 20,
                unread: 4,
            },
            ConversationCount {
                label_id: MY_LABEL_ID2.clone(),
                total: 400,
                unread: 124,
            },
        ];

        let expected_counts = [
            LocalConversationCount {
                id: labels[0],
                total: 20,
                unread: 4,
            },
            LocalConversationCount {
                id: labels[1],
                total: 400,
                unread: 124,
            },
        ];

        tx.create_or_update_conversation_counts(counts.iter())
            .expect("failed to creat counters");
        let db_counters = tx
            .get_conversation_counts()
            .expect("failed to get counters");
        assert!(db_counters.contains(&expected_counts[0]));
        assert!(db_counters.contains(&expected_counts[1]));

        let labels_with_counts = tx
            .label_by_type_ordered_with_conversation_count(LabelType::Label)
            .expect("failed to get label with type");
        assert_eq!(labels_with_counts.len(), 1);
        assert_eq!(labels_with_counts[0].id, expected_counts[0].id);
        assert_eq!(labels_with_counts[0].total_count, expected_counts[0].total);
        assert_eq!(
            labels_with_counts[0].unread_count,
            expected_counts[0].unread
        );
    });
}

lazy_static! {
    pub(super) static ref MY_ADDRESS_ID: AddressId = AddressId::from("MyAddressId");
    pub(super) static ref MY_LABEL_ID1: LabelId = LabelId::from("MyLabelID1");
    pub(super) static ref MY_LABEL_ID2: LabelId = LabelId::from("MyLabelID2");
    pub(super) static ref MY_ATTACHMENT_ID: AttachmentId = AttachmentId::from("MyAttachmentID1");
    pub(super) static ref MY_CONVERSATION_ID: ConversationId =
        ConversationId::from("MyConversationID");
}
pub(super) fn create_address_and_labels(tx: &mut MailSqliteConnectionMut) -> Vec<LocalLabelId> {
    tx.create_or_update_address(&test_address())
        .expect("failed to create address");
    let labels = [test_label1(), test_label2()];
    tx.create_remote_labels(labels.iter())
        .expect("failed to create labels");

    let r = tx
        .resolve_remote_label_ids(labels.iter().map(|l| &l.id))
        .expect("failed to resolve label ids");
    assert_eq!(r.len(), 2);
    r
}
pub(super) fn test_address() -> Address {
    Address {
        id: MY_ADDRESS_ID.clone(),
        email: "hello@world".to_string(),
        send: Default::default(),
        receive: Default::default(),
        status: AddressStatus::Enabled,
        domain_id: None,
        address_type: AddressType::Original,
        order: 0,
        display_name: "HelloWorld".to_string(),
        signature: "SIGNATURE".to_string(),
        keys: AddressKeys(Vec::new()),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList {
            min_epoch_id: None,
            max_epoch_id: None,
            expected_min_epoch_id: None,
            data: None,
            obsolescence_token: None,
            signature: None,
            revision: 0,
        },
    }
}

pub(super) fn test_label1() -> Label {
    Label {
        id: MY_LABEL_ID1.clone(),
        parent_id: None,
        name: "MyLabel".to_string(),
        path: None,
        color: "#000000".to_string(),
        label_type: LabelType::Label,
        notify: Default::default(),
        display: Default::default(),
        sticky: Default::default(),
        expanded: Default::default(),
        order: 0,
    }
}

pub(super) fn test_label2() -> Label {
    Label {
        id: MY_LABEL_ID2.clone(),
        parent_id: None,
        name: "MyFolder".to_string(),
        path: None,
        color: "#0000".to_string(),
        label_type: LabelType::Folder,
        notify: ProtonBoolean::True,
        display: Default::default(),
        sticky: Default::default(),
        expanded: ProtonBoolean::True,
        order: 1,
    }
}

pub(super) fn test_starred_label() -> Label {
    Label {
        id: LabelId::starred().clone(),
        parent_id: None,
        name: "Starred".to_string(),
        path: Some("Starred".to_string()),
        color: "#0000".to_string(),
        label_type: LabelType::System,
        notify: ProtonBoolean::False,
        display: Default::default(),
        sticky: Default::default(),
        expanded: ProtonBoolean::False,
        order: 2,
    }
}

pub(super) fn test_conversation(
    labels: impl IntoIterator<Item = ConversationLabels>,
    attachments: impl IntoIterator<Item = AttachmentMetadata>,
) -> Conversation {
    Conversation {
        id: MY_CONVERSATION_ID.clone(),
        order: 50,
        subject: "Hello World".to_string(),
        senders: vec![MessageAddress {
            address: "hello@world.com".to_string(),
            name: "HelloWorld".to_string(),
            ..Default::default()
        }],
        recipients: vec![
            MessageAddress {
                address: "foo@bar.com".to_string(),
                name: "Foo".to_string(),
                ..Default::default()
            },
            MessageAddress {
                address: "Bar@bar.com".to_string(),
                name: "bar".to_string(),
                ..Default::default()
            },
        ],
        num_messages: 10,
        num_unread: 4,
        num_attachments: 7,
        expiration_time: 1024,
        size: 4909,
        labels: labels.into_iter().collect(),
        display_snooze_reminder: false,
        attachments_metadata: attachments.into_iter().collect(),
        attachment_info: Default::default(),
    }
}
