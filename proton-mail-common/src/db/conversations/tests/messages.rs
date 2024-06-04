use crate::db::conversations::tests::conversations::{
    create_labels, test_conversation, test_label1, test_starred_label, MY_ADDRESS_ID,
    MY_CONVERSATION_ID, MY_LABEL_ID1, MY_LABEL_ID2,
};
use crate::db::conversations::tests::db_states::new_test_delete_db_state;
use crate::db::conversations::tests::utils::{
    conv_counts_as_map, find_conversation_label, msg_counts_as_map, prepare_and_patch_db_state,
};
use crate::db::{
    with_file_sqlite_db, with_tx, with_tx_core, LocalAttachmentMetadata, LocalConversationId,
    LocalInlineLabelInfo, LocalMessageBodyMetadata, LocalMessageCount, LocalMessageMetadata,
    MailSqliteConnectionMut,
};
use crate::exports::serde_json;
use lazy_static::lazy_static;
use proton_api_mail::domain::{
    AttachmentId, AttachmentMetadata, ConversationLabels, Disposition, LabelId, LabelType, Message,
    MessageAddress, MessageAttachment, MessageAttachmentHeaders, MessageCount, MessageFlags,
    MessageId, MessageMetadata, MimeType,
};
use proton_core_common::db::CoreSqliteConnectionMut;
use proton_crypto_inbox::attachment::KeyPackets;

use super::conversations::create_address;
use super::utils::prepare_db_state_core;

#[test]
fn test_create_message() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, test_create_message_dependencies_core);
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
            let expected = LocalMessageMetadata::from_message_metadata(
                id,
                conv_id,
                metadata,
                Some(vec![LocalInlineLabelInfo::from_label(
                    tx.resolve_remote_label_id(&MY_LABEL_ID1).unwrap().unwrap(),
                    &test_label1(),
                )]),
            );

            assert_eq!(db_metadata, expected);

            let message_labels = tx
                .get_message_labels(id)
                .expect("failed to get labels")
                .expect("must have value");
            assert_eq!(message_labels.len(), 1);
        });
    });
}

#[test]
fn test_create_message_with_attachments() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, test_create_message_dependencies_core);
        with_tx(&mut conn, |tx| {
            let attachment_metadata = AttachmentMetadata {
                id: AttachmentId::from("myattachment"),
                size: 80,
                name: "foo.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                disposition: Disposition::Inline,
            };
            let _ = test_create_message_dependencies(tx);
            let metadata =
                test_message_metadata([MY_LABEL_ID1.clone()], [attachment_metadata.clone()]);
            let id = tx
                .create_message_from_metadata(&metadata)
                .expect("failed to create message");

            let message_labels = tx
                .get_message_labels(id)
                .expect("failed to get labels")
                .expect("must have value");
            assert_eq!(message_labels.len(), 1);

            let attachments = tx
                .message_attachments_metadata(id)
                .expect("failed to get attachments")
                .expect("must have value");
            assert_eq!(attachments.len(), 1);
            let converted_attachment = LocalAttachmentMetadata::from_attachment_metadata(
                attachments[0].id,
                attachment_metadata.clone(),
            );
            assert_eq!(attachments[0], converted_attachment);

            let db_conversation = tx.get_message_metadata(id).unwrap().unwrap();
            assert_eq!(
                db_conversation.attachments.unwrap()[0],
                converted_attachment
            );
        });
    });
}

#[test]
fn test_update_message() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, test_create_message_dependencies_core);
        with_tx(&mut conn, |tx| {
            let conv_id = test_create_message_dependencies(tx);
            tx.create_remote_label(&test_starred_label()).unwrap();
            let metadata = test_message_metadata([MY_LABEL_ID1.clone()], []);
            let mut metadata_updated =
                test_message_metadata([MY_LABEL_ID2.clone(), LabelId::starred().clone()], []);
            metadata_updated.order = 20;
            metadata_updated.unread = true;
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
            let expected =
                LocalMessageMetadata::from_message_metadata(id, conv_id, metadata_updated, None);
            assert_eq!(db_metadata, expected);
            assert!(db_metadata.starred);

            let message_labels = tx
                .get_message_labels(id)
                .expect("failed to get labels")
                .expect("must have value");
            assert_eq!(message_labels.len(), 2);
        });
    });
}

#[test]
fn test_message_counts() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, create_address);
        with_tx(&mut conn, |tx| {
            let labels = create_labels(tx);
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
    });
}

#[test]
pub fn test_delete_local_message() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            // Deleting a message must
            // * Update conversation counters
            // * Update conversation labels
            // * Update message counters
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

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
    });
}

#[test]
pub fn test_message_metadata_list() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (_, state_map) = prepare_and_patch_db_state(tx, state.clone());
            let messages = tx
                .message_metadata_list(*state_map.labels.get(&MY_LABEL_ID1).unwrap(), 10)
                .unwrap();
            assert_eq!(messages.len(), 3);
            assert!(messages[0].time > messages[1].time);
            assert!(messages[1].time > messages[2].time);
        });
    });
}

#[test]
pub fn test_delete_local_message_does_not_change_conv_unread_count() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

            // Delete 2nd message from 1st conversation.
            let message = &state.messages[0];
            let local_id = *state_map.messages.get(&message.id).unwrap();
            tx.mark_local_message_as_deleted(local_id)
                .expect("failed to mark local message as deleted");
            let local_label_id = state_map.labels.get(&MY_LABEL_ID1).unwrap();

            let conv_counts = conv_counts_as_map(tx);
            let label_conv_counts = conv_counts.get(local_label_id).unwrap();
            assert_eq!(label_conv_counts.unread, 1);
        });
    });
}

#[test]
pub fn test_undelete_local_message() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            // Same as test_delete_local_message, but undo the operations
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

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

                tx.unmark_local_message_as_deleted(local_id)
                    .expect("failed to undelete message");

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
                        remote_conversation_label.context_num_messages,
                    );
                    assert_eq!(
                        local_conv.num_unread,
                        remote_conversation_label.context_num_unread,
                    );
                    assert_eq!(local_conv.time, state.messages[3].time);
                    assert_eq!(local_conv.size, remote_conversation_label.context_size,);
                    assert_eq!(
                        local_conv.num_attachments,
                        remote_conversation_label.context_num_attachments,
                    );
                    assert_eq!(local_conv.num_messages, state.conversations[0].num_messages,);

                    let local_conv = tx.get_conversation(local_conv_id).unwrap().unwrap();

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
                    .filter(|m| m.conversation_id == state.conversations[0].id)
                    .map(|m| *state_map.messages.get(&m.id).unwrap())
                    .collect::<Vec<_>>();
                tx.mark_local_messages_as_deleted(ids.iter().cloned())
                    .expect("failed to delete messages");
                tx.unmark_local_messages_as_deleted(ids.into_iter())
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
                    assert_eq!(conv_count.total, start_conv_count.total);
                    assert_eq!(msg_count.total, start_msg_count.total);
                }

                // Conversation should be deleted
                assert!(tx.get_conversation(local_conv_id).unwrap().is_some());
            }
        });
    });
}

#[test]
fn test_create_message_and_body() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, test_create_message_dependencies_core);
        with_tx(&mut conn, |tx| {
            test_create_message_dependencies(tx);
            let message = Message {
                metadata: test_message_metadata([MY_LABEL_ID1.clone()], []),
                header: "my headers".to_string(),
                parsed_headers: velcro::hash_map! {
                    "foo".to_owned(): serde_json::Value::String("bar".to_owned()),
                    "zeta".to_string():serde_json::Value::String("gama".to_string()),
                },
                body: "my_message".to_string(),
                mime_type: MimeType::TextPlain,
                attachments: vec![],
            };
            let id = tx
                .create_message_from_metadata(&message.metadata)
                .expect("failed to create message");
            let db_body_after_insert = tx.create_or_update_message_body(&message).unwrap();

            assert_eq!(id, db_body_after_insert.id);

            let db_message_body = tx
                .message_body(id)
                .expect("failed to get message")
                .expect("must have a value");

            assert_eq!(db_body_after_insert, db_message_body);

            let expected = LocalMessageBodyMetadata::from_message(id, &message);

            assert_eq!(db_message_body, expected);
        });
    });
}

#[test]
fn test_update_message_and_body() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, test_create_message_dependencies_core);
        with_tx(&mut conn, |tx| {
            test_create_message_dependencies(tx);
            let mut message = Message {
                metadata: test_message_metadata([MY_LABEL_ID1.clone()], []),
                header: "my headers".to_string(),
                parsed_headers: velcro::hash_map! {
                    "foo".to_owned(): serde_json::Value::String("bar".to_owned()),
                    "zeta".to_string():serde_json::Value::String("gama".to_string()),
                },
                body: "my_message".to_string(),
                mime_type: MimeType::TextPlain,
                attachments: vec![],
            };
            let id = tx
                .create_message_from_metadata(&message.metadata)
                .expect("failed to create message");
            tx.create_or_update_message_body(&message).unwrap();

            // Update the body
            message.parsed_headers.insert(
                "marco".to_owned(),
                serde_json::Value::String("polo".to_owned()),
            );
            message.header = "new header".to_owned();
            message.body = "new body type".to_owned();
            message.mime_type = MimeType::TextHTML;

            tx.create_or_update_message_body(&message).unwrap();

            let db_message_body = tx
                .message_body(id)
                .expect("failed to get message")
                .expect("must have a value");

            let expected = LocalMessageBodyMetadata::from_message(id, &message);

            assert_eq!(db_message_body, expected);
        });
    });
}

#[test]
fn test_create_message_and_body_with_attachments() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, test_create_message_dependencies_core);
        with_tx(&mut conn, |tx| {
            let attachment_id = AttachmentId::from("attachment");
            test_create_message_dependencies(tx);
            let message = Message {
                metadata: test_message_metadata(
                    [MY_LABEL_ID1.clone()],
                    [AttachmentMetadata {
                        id: attachment_id.clone(),
                        size: 1024,
                        name: "fooo".to_string(),
                        mime_type: "text/html".to_owned(),
                        disposition: Disposition::Inline,
                    }],
                ),
                header: "my headers".to_string(),
                parsed_headers: velcro::hash_map! {
                    "foo".to_owned(): serde_json::Value::String("bar".to_owned()),
                    "zeta".to_string():serde_json::Value::String("gama".to_string()),
                },
                body: "my_message".to_string(),
                mime_type: MimeType::TextPlain,
                attachments: vec![MessageAttachment {
                    id: attachment_id.clone(),
                    name: "fooo".to_owned(),
                    size: 1024,
                    mime_type: "text/html".to_owned(),
                    disposition: Disposition::Inline,
                    key_packets: KeyPackets::from("packets"),
                    signature: None,
                    enc_signature: None,
                    headers: MessageAttachmentHeaders {
                        content_disposition: "inline".to_string(),
                        content_id: Some("mycontent_id".to_owned()),
                        content_transfer_encoding: Some("base64".to_string()),
                        image_width: Some("1280".to_owned()),
                        image_height: Some("720".to_owned()),
                    },
                }],
            };
            let id = tx
                .create_message_from_metadata(&message.metadata)
                .expect("failed to create message");
            tx.create_or_update_message_body(&message).unwrap();
            let local_attachments = tx.attachments_for_message(id).expect("must have value");
            let local_attachment = local_attachments.first().unwrap();

            assert_eq!(
                local_attachment.content_id,
                message.attachments[0].headers.content_id
            );
            assert_eq!(
                local_attachment.content_transfer_encoding,
                message.attachments[0].headers.content_transfer_encoding
            );
            assert_eq!(
                local_attachment.pm_image_width,
                message.attachments[0].headers.image_width
            );
            assert_eq!(
                local_attachment.pm_image_height,
                message.attachments[0].headers.image_height
            );
        });
    });
}

lazy_static! {
    pub(super) static ref MY_MESSAGE_ID: MessageId = MessageId::from("MyMessageId");
}

fn test_create_message_dependencies_core(tx_core: &mut CoreSqliteConnectionMut) {
    create_address(tx_core);
}

fn test_create_message_dependencies(tx: &mut MailSqliteConnectionMut) -> LocalConversationId {
    create_labels(tx);
    let conversation = test_conversation(
        [ConversationLabels {
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
        flags: MessageFlags::AUTO | MessageFlags::PHISHING_AUTO,
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
