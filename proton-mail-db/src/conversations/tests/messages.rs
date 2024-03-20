use crate::conversations::tests::conversations::{
    create_address_and_labels, test_conversation, test_starred_label, MY_ADDRESS_ID,
    MY_CONVERSATION_ID, MY_LABEL_ID1, MY_LABEL_ID2,
};
use crate::conversations::tests::db_states::new_test_delete_db_state;
use crate::conversations::tests::utils::{
    conv_counts_as_map, find_conversation_label, msg_counts_as_map, prepare_and_patch_db_state,
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
            .create_message_from_metadata(&metadata)
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
fn test_update_message() {
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let conv_id = test_create_message_dependencies(tx);
        tx.create_remote_label(&test_starred_label()).unwrap();
        let metadata = test_message_metadata([MY_LABEL_ID1.clone()], []);
        let mut metadata_updated =
            test_message_metadata([MY_LABEL_ID2.clone(), LabelId::starred().clone()], []);
        metadata_updated.order = 20;
        metadata_updated.unread = ProtonBoolean::True;
        metadata_updated.label_ids.push(LabelId::starred().clone());
        let id = tx
            .create_message_from_metadata(&metadata)
            .expect("failed to create message");
        tx.update_message_from_metadata(&metadata_updated)
            .expect("failed to update message");
        let db_metadata = tx
            .get_message_metadata(id)
            .expect("failed to get message")
            .expect("must have a value");
        let expected = LocalMessageMetadata::from_message_metadata(id, conv_id, metadata_updated);
        assert_eq!(db_metadata, expected);
        assert!(db_metadata.starred);

        let message_labels = tx
            .get_message_labels(id)
            .expect("failed to get labels")
            .expect("must have value");
        assert_eq!(message_labels.len(), 2);
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
            .label_by_type_ordered_with_message_count(LabelType::Label)
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

#[test]
pub fn test_delete_local_message() {
    //TODO: deleting an unread message should not cause unread counter to change
    //TODO: deleting all messages from a conversation should delete conversation
    let (mut conn, _, _d) = new_test_connection();
    with_tx(&mut conn, |tx| {
        // Deleting a message must
        // * Update conversation counters
        // * Update conversation labels
        // * Update message counters

        let state = new_test_delete_db_state();
        let (state, state_map) = prepare_and_patch_db_state(tx, state);

        let local_conv_id = *state_map
            .conversations
            .get(&state.conversations[0].id)
            .unwrap();
        {
            // Delete 3rd message from 1st conversation.
            let message = &state.messages[2];
            let local_id = *state_map.messages.get(&message.id).unwrap();
            tx.mark_local_message_as_deleted(local_id)
                .expect("failed to mark local message as deleted");

            let conv_counts = conv_counts_as_map(tx);
            let msg_counts = msg_counts_as_map(tx);

            for label in &message.label_ids {
                let local_label_id = *state_map
                    .labels
                    .get(label)
                    .expect("Failed to resolve label");
                let conv_count = conv_counts.get(&local_label_id).unwrap();
                let start_conv_count = state_map.conversation_counts.get(label).unwrap();
                let start_msg_count = state_map.message_counts.get(label).unwrap();

                let local_conv = tx
                    .get_conversation_with_context(local_conv_id, local_label_id)
                    .unwrap()
                    .unwrap();
                let remote_conversation_label =
                    find_conversation_label(&state.conversations[0], label);

                assert_eq!(
                    local_conv.num_messages_ctx,
                    remote_conversation_label.context_num_messages - 1
                );
                assert_eq!(
                    local_conv.num_unread,
                    remote_conversation_label.context_num_unread - 1
                );
                assert_eq!(local_conv.time, state.messages[3].time);
                assert_eq!(
                    local_conv.size,
                    remote_conversation_label.context_size - message.size
                );
                assert_eq!(
                    local_conv.num_attachments,
                    remote_conversation_label.context_num_attachments
                        - message.num_attachments as u64
                );
                assert_eq!(
                    local_conv.num_messages,
                    state.conversations[0].num_messages - 1
                );

                let local_conv = tx.get_conversation(local_conv_id).unwrap().unwrap();

                assert_eq!(
                    local_conv.num_messages,
                    state.conversations[0].num_messages - 1
                );
                assert_eq!(local_conv.num_unread, state.conversations[0].num_unread - 1);

                let msg_count = msg_counts.get(&local_label_id).unwrap();
                assert_eq!(msg_count.total, start_msg_count.total - 1);
                assert_eq!(msg_count.unread, start_msg_count.unread - 1);

                assert_eq!(conv_count.total, start_conv_count.total);
                // Conversation 1 & 2 have only one unread message on different labels and we removed
                // the unread message from label1.
                assert_eq!(conv_count.unread, 0);
            }
        }

        {
            // Delete remaining messages from first conversation
            let ids = state
                .messages
                .iter()
                .filter(|m| m.conversation_id == state.conversations[0].id)
                .map(|m| *state_map.messages.get(&m.id).unwrap())
                .collect::<Vec<_>>();
            tx.mark_local_messages_as_deleted(ids.into_iter())
                .expect("failed to delete messages");

            let conv_counts = conv_counts_as_map(tx);
            let msg_counts = msg_counts_as_map(tx);

            for label in &state.conversations[0].labels {
                let local_label_id = *state_map
                    .labels
                    .get(&label.id)
                    .expect("Failed to resolve label");
                let conv_count = conv_counts.get(&local_label_id).unwrap();
                let msg_count = msg_counts.get(&local_label_id).unwrap();
                let start_conv_count = state_map.conversation_counts.get(&label.id).unwrap();
                let start_msg_count = state_map.message_counts.get(&label.id).unwrap();

                // Conversation should no longer exist
                assert_eq!(conv_count.total, start_conv_count.total - 1);
                if label.id == state.labels[0].id {
                    assert_eq!(msg_count.total, start_msg_count.total - 3);
                } else {
                    assert_eq!(msg_count.total, start_msg_count.total - 1);
                }
            }

            // Conversation should be deleted
            assert!(tx.get_conversation(local_conv_id).unwrap().is_none());
        }
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
