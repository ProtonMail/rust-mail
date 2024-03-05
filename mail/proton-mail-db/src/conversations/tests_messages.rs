use crate::conversations::tests_conversations::{
    create_address_and_labels, test_conversation, MY_ADDRESS_ID, MY_CONVERSATION_ID, MY_LABEL_ID1,
    MY_LABEL_ID2,
};
use crate::{
    new_test_connection, with_tx, LocalConversationId, LocalMessageCount, LocalMessageMetadata,
    MailSqliteConnectionMut,
};
use lazy_static::lazy_static;
use proton_api_mail::domain::{
    AttachmentMetadata, ConversationLabels, LabelId, LabelType, MessageAddress, MessageCount,
    MessageId, MessageMetadata,
};
use proton_api_mail::proton_api_core::domain::ProtonBoolean;

#[test]
fn test_create_message() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let conv_id = test_create_message_dependencies(tx);
        let metadata = test_message_metadata([MY_LABEL_ID1.clone()], []);
        let id = tx
            .creat_message_from_metadata(&metadata)
            .expect("failed to create message");
        let db_metadata = tx
            .get_message_metadata(id)
            .expect("failed to get message")
            .expect("must have a value");
        let expected = LocalMessageMetadata::from_message_metadata(id, conv_id, metadata);
        assert_eq!(db_metadata, expected);

        let message_labels = tx
            .get_message_labels(id)
            .expect("failed to get labels")
            .expect("must have value");
        assert_eq!(message_labels.len(), 1);
    });
}

#[test]
fn test_message_counts() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let labels = create_address_and_labels(tx);
        let counts = [
            MessageCount {
                label_id: MY_LABEL_ID1.clone(),
                total: 20,
                unread: 4,
            },
            MessageCount {
                label_id: MY_LABEL_ID2.clone(),
                total: 400,
                unread: 124,
            },
        ];

        let expected_counts = [
            LocalMessageCount {
                id: labels[0],
                total: 20,
                unread: 4,
            },
            LocalMessageCount {
                id: labels[1],
                total: 400,
                unread: 124,
            },
        ];

        tx.create_or_update_message_counts(counts.iter())
            .expect("failed to creat counters");
        let db_counters = tx.get_message_counts().expect("failed to get counters");
        assert!(db_counters.contains(&expected_counts[0]));
        assert!(db_counters.contains(&expected_counts[1]));

        let labels_with_counts = tx
            .get_local_label_by_type_ordered_with_message_count(LabelType::Label)
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
    pub(super) static ref MY_MESSAGE_ID: MessageId = MessageId::from("MyMessageId");
}

fn test_create_message_dependencies(tx: &mut MailSqliteConnectionMut) -> LocalConversationId {
    create_address_and_labels(tx);
    let conversation = test_conversation(
        [ConversationLabels {
            id: MY_LABEL_ID1.clone(),
            context_num_unread: 0,
            context_num_messages: 0,
            context_time: 0,
            context_size: 0,
            context_num_attachments: 0,
        }],
        [],
    );
    tx.create_conversation(&conversation)
        .expect("failed to create conversation")
}

fn test_message_metadata(
    label_ids: impl IntoIterator<Item = LabelId>,
    attachments: impl IntoIterator<Item = AttachmentMetadata>,
) -> MessageMetadata {
    MessageMetadata {
        id: MY_MESSAGE_ID.clone(),
        conversation_id: MY_CONVERSATION_ID.clone(),
        order: 1,
        address_id: MY_ADDRESS_ID.clone(),
        label_ids: label_ids.into_iter().collect(),
        external_id: None,
        subject: "Hello ".to_string(),
        sender: MessageAddress {
            address: "hello@world.com".to_string(),
            name: "hello".to_string(),
            is_proton: Default::default(),
            display_sender_image: Default::default(),
            is_simple_login: Default::default(),
            bimi_selector: None,
        },
        to_list: vec![],
        cc_list: vec![],
        bcc_list: vec![],
        reply_tos: vec![],
        flags: 30,
        time: 100,
        size: 1024,
        unread: Default::default(),
        is_replied: ProtonBoolean::True,
        is_replied_all: Default::default(),
        is_forwarded: ProtonBoolean::True,
        expiration_time: 10000,
        num_attachments: 24,
        attachments_metadata: attachments.into_iter().collect(),
    }
}
