use crate::db::conversations::tests::db_states::{
    new_test_delete_db_state, new_test_label_db_state,
    new_test_label_db_state_label_with_existing_labels, new_test_unread_db_state,
};
use crate::db::conversations::tests::utils::{
    conv_counts_as_map, message_counts_for_conversation, msg_counts_as_map,
    prepare_and_patch_db_state, prepare_and_patch_db_state_and_skip,
};
use crate::db::conversations::types::LocalConversation;
use crate::db::{
with_file_sqlite_db, with_tx, with_tx_core, LabelColor, LocalAttachmentMetadata, LocalConversationCount,
    LocalInlineLabelInfo, LocalLabelId, MailSqliteConnectionMut,
};
use lazy_static::lazy_static;
use proton_api_mail::domain::{
    AttachmentId, AttachmentMetadata, Conversation, ConversationCount, ConversationId,
    ConversationLabels, Disposition, Label, LabelId, LabelType, MessageAddress,
};
use proton_api_mail::exports::crypto::domain::AddressKeys;
use proton_api_mail::exports::tracing;
use proton_api_mail::proton_api_core::domain::{
    Address, AddressId, AddressSignedKeyList, AddressStatus, AddressType,
};
use proton_core_common::db::CoreSqliteConnectionMut;
use tracing_test::traced_test;

use super::utils::prepare_db_state_core;

#[test]
fn test_conversation_create_no_labels() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, create_address);
        with_tx(&mut conn, |tx| {
            create_labels(tx);
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
        context_expiration_time: 0,
        context_snooze_time: 0,
    };
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, create_address);
        with_tx(&mut conn, |tx| {
            create_labels(tx);
            tx.create_remote_label(&test_starred_label()).unwrap();

            // Add starred label, should gain starred attribute.
            let conv = test_conversation([conv_label.clone()], []);
            let id = tx
                .create_conversation(&conv)
                .expect("failed to create conversation");

            {
                let local_conversation =
                    LocalConversation::from_conversation(id, conv.clone(), None);
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
                let local_conversation =
                    LocalConversation::from_conversation(id, conv.clone(), None);
                let db_conversation = tx
                    .get_conversation(id)
                    .expect("failed to get conversation")
                    .expect("should have value");
                assert_eq!(local_conversation, db_conversation);
                assert!(!local_conversation.starred);
                assert!(!db_conversation.starred);
            }
        });
    });
}

#[test]
fn test_conversation_create_with_labels() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, create_address);
        with_tx(&mut conn, |tx| {
            let local_label_ids = create_labels(tx);
            let conv = test_conversation(
                [
                    ConversationLabels {
                        id: MY_LABEL_ID1.clone(),
                        context_num_unread: 1,
                        context_num_messages: 2,
                        context_time: 3,
                        context_size: 4,
                        context_num_attachments: 5,
                        context_expiration_time: 6,
                        context_snooze_time: 21,
                    },
                    ConversationLabels {
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
    });
}

#[test]
fn test_conversation_create_with_attachment() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, create_address);
        with_tx(&mut conn, |tx| {
            create_labels(tx);
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
    });
}

#[test]
fn test_conversation_create_with_attachment_and_label() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, create_address);
        with_tx(&mut conn, |tx| {
            let local_labels = create_labels(tx);
            let conv = test_conversation(
                [ConversationLabels {
                    id: MY_LABEL_ID1.clone(),
                    context_num_unread: 1,
                    context_num_messages: 2,
                    context_time: 3,
                    context_size: 4,
                    context_num_attachments: 5,
                    context_expiration_time: 6,
                    context_snooze_time: 7,
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
    });
}

#[test]
fn test_conversation_update() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, create_address);
        with_tx(&mut conn, |tx| {
            let local_label_ids = create_labels(tx);
            let conv = test_conversation(
                [ConversationLabels {
                    id: MY_LABEL_ID2.clone(),
                    context_num_unread: 7,
                    context_num_messages: 8,
                    context_time: 9,
                    context_size: 10,
                    context_num_attachments: 11,
                    context_expiration_time: 12,
                    context_snooze_time: 21,
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
                    context_expiration_time: 6,
                    context_snooze_time: 7,
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
                label,
                conv.clone(),
                Some(vec![LocalInlineLabelInfo {
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
            assert_eq!(
                local_conversation, db_conversation,
                "conversation with context (LabelId={label}) do not match"
            );
        }
    });
}

#[test]
fn test_conversation_create_with_attachment() {
    let (mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        create_labels(tx);
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
    let (mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let local_labels = create_labels(tx);
        let conv = test_conversation(
            [ConversationLabels {
                id: MY_LABEL_ID1.clone(),
                context_num_unread: 1,
                context_num_messages: 2,
                context_time: 3,
                context_size: 4,
                context_num_attachments: 5,
                context_expiration_time: 6,
                context_snooze_time: 7,
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
    let (mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let local_label_ids = create_labels(tx);
        let conv = test_conversation(
            [ConversationLabels {
                id: MY_LABEL_ID2.clone(),
                context_num_unread: 7,
                context_num_messages: 8,
                context_time: 9,
                context_size: 10,
                context_num_attachments: 11,
                context_expiration_time: 12,
                context_snooze_time: 21,
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
                context_expiration_time: 6,
                context_snooze_time: 7,
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
            Some(vec![LocalInlineLabelInfo {
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
fn test_conversation_undelete_all_mail() {
    // Same as test_conversation_delete, but undoing the deletions should restore all the state
    // back to the initial values.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());
            let all_mail_label = tx
                .resolve_remote_label_id(LabelId::all_mail())
                .unwrap()
                .unwrap();

            let local_conv_id1 = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_conv_id2 = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
            tx.mark_conversations_as_deleted(
                all_mail_label,
                [local_conv_id1, local_conv_id2].into_iter(),
            )
            .expect("failed to mark as deleted");

            tx.unmark_conversations_as_deleted(
                all_mail_label,
                [local_conv_id1, local_conv_id2].into_iter(),
            )
            .expect("failed to mark conversations as undeleted");

            // Check conversation counts
            {
                let conv_counts = conv_counts_as_map(tx);
                // Check conversation label1 values
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
                // Check conversation label2 values
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
                let message_counts = msg_counts_as_map(tx);

                // Check label1
                {
                    let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = message_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
                // Check label2
                {
                    let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = message_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
            }
        });
    });
}

#[test]
fn test_conversation_delete_all_mail() {
    // Simulate conversation delete from all mail, all messages for the conversation a
    // are deleted.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());
            let all_mail_label = tx
                .resolve_remote_label_id(LabelId::all_mail())
                .unwrap()
                .unwrap();

            // Deleting a conversation must
            // * Update conversation counters
            // * Update message counters

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
            tx.mark_conversation_as_deleted(all_mail_label, local_conv_id)
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
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread - 1,);
                    assert_eq!(label_counts.total, start_label_counts.total - 1,);
                }
                // Check conversation label2 values
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
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
            tx.mark_conversation_as_deleted(all_mail_label, local_conv_id)
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
    });
}

#[test]
fn test_conversation_delete() {
    // Simulate conversation according to API expectations, only delete conversations in that label.
    // If conversation has messages in other labels, it must still exist.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, move |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());
            // Deleting a conversation must
            // * Update conversation counters
            // * Update message counters

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
            tx.mark_conversation_as_deleted(local_label_id1, local_conv_id)
                .expect("failed to mark as deleted");

            let db_conversation = tx
                .get_conversation(local_conv_id)
                .expect("failed to get conversation")
                .unwrap();

            // No more unread messages
            assert_eq!(db_conversation.num_unread, 0);
            // Should only have one message in other label
            assert_eq!(db_conversation.num_messages, 1);
            assert_eq!(db_conversation.size, state.messages[1].size);
            assert_eq!(
                db_conversation.num_attachments,
                state.messages[1].num_attachments as u64
            );

            assert!(tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .unwrap()
                .is_none());
            assert!(tx
                .get_conversation_with_context(local_conv_id, local_label_id2)
                .unwrap()
                .is_some());

            // Check conversation counts
            {
                let conv_counts = conv_counts_as_map(tx);
                // Check conversation label1 values, conversation should have been removed.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread - 1);
                    assert_eq!(label_counts.total, start_label_counts.total - 1);
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
                // Check label2 - should be unchanged.
                {
                    let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = message_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
            }

            // Deleting conv1 in label 2  should remove all traces of the  conversation
            tx.mark_conversation_as_deleted(local_label_id2, local_conv_id)
                .expect("failed to mark conv as deleted");

            assert!(tx
                .get_conversation_with_context(local_conv_id, local_label_id2)
                .unwrap()
                .is_none());

            {
                let db_conv = tx
                    .get_conversation(local_conv_id)
                    .expect("failed to get conversation");
                assert!(db_conv.is_none());
            }

            // Check conversation counts
            {
                let conv_counts = conv_counts_as_map(tx);
                // Check conversation label1 values, should be empty
                {
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, 0);
                    assert_eq!(label_counts.total, 0);
                }
                // Check conversation label2 values, should be missing one conversation.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = conv_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total - 1);
                }
            }

            // Check message counts
            {
                let message_counts = msg_counts_as_map(tx);

                // Check label1
                {
                    let label_counts = message_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, 0);
                    assert_eq!(label_counts.total, 0);
                }
                // Check label2 - should be missing one message.
                {
                    let start_label_counts = state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = message_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total - 1);
                }
            }
        });
    });
}

#[test]
fn test_conversation_undelete() {
    // Same as test_conversation_delete, but checks for reverse operations.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

            // Deleting a conversation must
            // * Update conversation counters
            // * Update message counters

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
            tx.mark_conversation_as_deleted(local_label_id1, local_conv_id)
                .expect("failed to mark as deleted");
            tx.mark_conversation_as_deleted(local_label_id2, local_conv_id)
                .expect("failed to mark as deleted");

            tx.unmark_conversation_as_deleted(local_label_id2, local_conv_id)
                .expect("Failed to mark as undeleted");
            tx.unmark_conversation_as_deleted(local_label_id1, local_conv_id)
                .expect("Failed to mark as undeleted");

            assert!(tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .is_some());
            assert!(tx
                .get_conversation_with_context(local_conv_id, local_label_id2)
                .expect("failed to get conversation")
                .is_some());

            let db_conversation = tx
                .get_conversation(local_conv_id)
                .expect("failed to get conversation")
                .unwrap();

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
                let conv_counts = conv_counts_as_map(tx);
                // Check conversation label1 values, should match original state.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
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
                let message_counts = msg_counts_as_map(tx);

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
        });
    });
}

#[test]
fn test_conversation_counts() {
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, create_address);
        with_tx(&mut conn, |tx| {
            let labels = create_labels(tx);
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
    });
}

#[test]
fn test_conversation_mark_read_no_message_metadata() {
    // Mark conversation as read without message metadata.
    let state = new_test_unread_db_state();
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state_and_skip(tx, state.clone(), true);

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
            tx.mark_conversation_read(local_conv_id)
                .expect("failed to mark as read");

            let db_conversation = tx
                .get_conversation(local_conv_id)
                .expect("failed to get conversation")
                .unwrap();

            // No more unread messages
            assert_eq!(db_conversation.num_unread, 0);
            assert_eq!(db_conversation.num_messages, 4);

            // Check conversation counts
            {
                let conv_counts = conv_counts_as_map(tx);
                // Check conversation label1 values, conversation should have been removed.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread - 1);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
                // Check conversation label2 values - should be unchanged.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = conv_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread - 1);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
            }
        });
    });
}

#[test]
fn test_conversation_mark_read() {
    // Mark conversation as read and update all conversation / message counts
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_unread_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
            tx.mark_conversation_read(local_conv_id)
                .expect("failed to mark as read");

            let db_conversation = tx
                .get_conversation(local_conv_id)
                .expect("failed to get conversation")
                .unwrap();

            // No more unread messages
            assert_eq!(db_conversation.num_unread, 0);
            assert_eq!(db_conversation.num_messages, 4);

            // Check conversation counts
            {
                let conv_counts = conv_counts_as_map(tx);
                // Check conversation label1 values, conversation should have been removed.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread - 1);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
                // Check conversation label2 values - should be unchanged.
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = conv_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread - 1);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
            }

            // Check message counts
            {
                let message_counts = msg_counts_as_map(tx);

                // Check label1
                {
                    let (unread, _) = message_counts_for_conversation(
                        &state.messages,
                        &state.conversations[0].id,
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
        });
    });
}

#[test]
fn test_conversation_mark_unread_no_metadata() {
    // Mark conversation as read and then mark it unread, since we don't have message
    // metadata we should mark the current conversation label only as unread.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_unread_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state_and_skip(tx, state.clone(), true);

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
            tx.mark_conversation_read(local_conv_id)
                .expect("failed to mark as read");
            tx.mark_conversation_unread(local_label_id1, local_conv_id)
                .expect("failed to mark as unread");

            let db_conversation = tx
                .get_conversation(local_conv_id)
                .expect("failed to get conversation")
                .unwrap();

            // There should be 1 unread message.
            assert_eq!(db_conversation.num_unread, 1);
            assert_eq!(db_conversation.num_messages, 4);

            // Check conversation counts match original values.
            {
                let conv_counts = conv_counts_as_map(tx);
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
                {
                    // Label2 should have no unread messages since the message in conv 1 is not the latest.
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = conv_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread - 1);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
            }
        });
    });
}

#[test]
fn test_conversation_mark_unread() {
    // Mark conversation as read and then mark it unread, only the LATEST message in the
    // conversation should be marked read.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_delete_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let state = new_test_unread_db_state();
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
            tx.mark_conversation_read(local_conv_id)
                .expect("failed to mark as read");
            tx.mark_conversation_unread(local_label_id1, local_conv_id)
                .expect("failed to mark as unread");

            let db_conversation = tx
                .get_conversation(local_conv_id)
                .expect("failed to get conversation")
                .unwrap();

            // There should be 1 unread message.
            assert_eq!(db_conversation.num_unread, 1);
            assert_eq!(db_conversation.num_messages, 4);

            // Check conversation counts match original values.
            {
                let conv_counts = conv_counts_as_map(tx);
                {
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
                {
                    // Label2 should have no unread messages since the message in conv 1 is not the latest.
                    let start_label_counts =
                        state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
                    let label_counts = conv_counts.get(&local_label_id2).unwrap();
                    assert_eq!(label_counts.unread, start_label_counts.unread - 1);
                    assert_eq!(label_counts.total, start_label_counts.total);
                }
            }

            // Check message counts, only one message should be unread
            {
                let message_counts = msg_counts_as_map(tx);

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
        });
    });
}

#[test]
fn test_conversation_label_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_label_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");

            let db_conversation = tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .unwrap();

            // There should be 1 unread message.
            assert_eq!(db_conversation.num_unread, 1);
            assert_eq!(db_conversation.num_messages_ctx, 3);
            assert_eq!(db_conversation.num_attachments, 1);
            assert_eq!(
                db_conversation.size,
                state.messages.iter().fold(0, |x, m| x + m.size)
            );
            assert_eq!(
                db_conversation.time,
                state.messages.iter().fold(0, |x, m| x.max(m.time))
            );
            assert_eq!(
                db_conversation.expiration_time,
                state
                    .messages
                    .iter()
                    .fold(0, |x, m| x.max(m.expiration_time))
            );
            assert_eq!(
                db_conversation.snooze_time,
                state.messages.iter().fold(0, |x, m| x.max(m.snooze_time))
            );

            // Check conversation counts have the new conversation.
            {
                let conv_counts = conv_counts_as_map(tx);
                let label_counts = conv_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 1);
                assert_eq!(label_counts.total, 1);
            }

            // Check message counts, only one message should be unread
            {
                let message_counts = msg_counts_as_map(tx);
                let label_counts = message_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 1);
                assert_eq!(label_counts.total, 3);
            }
        });
    });
}

#[test]
fn test_conversation_double_label_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation twice and check
    // the changes are not duplicated.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_label_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");

            let db_conversation = tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .unwrap();

            // There should be 1 unread message.
            assert_eq!(db_conversation.num_unread, 1);
            assert_eq!(db_conversation.num_messages_ctx, 3);
            assert_eq!(db_conversation.num_attachments, 1);
            assert_eq!(
                db_conversation.size,
                state.messages.iter().fold(0, |x, m| x + m.size)
            );
            assert_eq!(
                db_conversation.time,
                state.messages.iter().fold(0, |x, m| x.max(m.time))
            );
            assert_eq!(
                db_conversation.expiration_time,
                state
                    .messages
                    .iter()
                    .fold(0, |x, m| x.max(m.expiration_time))
            );
            assert_eq!(
                db_conversation.snooze_time,
                state.messages.iter().fold(0, |x, m| x.max(m.snooze_time))
            );

            // Check conversation counts have the new conversation.
            {
                let conv_counts = conv_counts_as_map(tx);
                let label_counts = conv_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 1);
                assert_eq!(label_counts.total, 1);
            }

            // Check message counts, only one message should be unread
            {
                let message_counts = msg_counts_as_map(tx);
                let label_counts = message_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 1);
                assert_eq!(label_counts.total, 3);
            }
        });
    });
}

#[test]
#[traced_test]
fn test_conversation_label_partially() {
    // Label conversation with a label where one of the messages already has been labeled
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_label_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let mut state = state.clone();
            state.messages[1].label_ids.push(MY_LABEL_ID1.clone());
            state.conversations[0].labels.push(ConversationLabels {
                id: MY_LABEL_ID1.clone(),
                context_num_unread: 0,
                context_num_messages: 0,
                context_time: 0,
                context_size: 0,
                context_num_attachments: 0,
                context_expiration_time: 0,
                context_snooze_time: 0,
            });
            let (state, state_map) = prepare_and_patch_db_state(tx, state);

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");

            let db_conversation = tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .unwrap();

            // There should be 1 unread message.
            assert_eq!(db_conversation.num_unread, 1);
            assert_eq!(db_conversation.num_messages_ctx, 3);
            assert_eq!(db_conversation.num_attachments, 1);
            assert_eq!(
                db_conversation.size,
                state.messages.iter().fold(0, |x, m| x + m.size)
            );
            assert_eq!(
                db_conversation.time,
                state.messages.iter().fold(0, |x, m| x.max(m.time))
            );
            assert_eq!(
                db_conversation.expiration_time,
                state
                    .messages
                    .iter()
                    .fold(0, |x, m| x.max(m.expiration_time))
            );
            assert_eq!(
                db_conversation.snooze_time,
                state.messages.iter().fold(0, |x, m| x.max(m.snooze_time))
            );

            // Check conversation counts have the new conversation.
            {
                let conv_counts = conv_counts_as_map(tx);
                let label_counts = conv_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 1);
                assert_eq!(label_counts.total, 1);
            }

            // Check message counts, only one message should be unread
            {
                let message_counts = msg_counts_as_map(tx);
                let label_counts = message_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 1);
                assert_eq!(label_counts.total, 3);
            }
        });
    });
}

#[test]
fn test_conversation_label_without_message_metadata() {
    // Label a conversation with a label that was never assigned without having any message metadata
    // present.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_label_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state_and_skip(tx, state.clone(), true);

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");

            let db_conversation = tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .unwrap();

            // Because we have no message metadata, all these values should be empty
            assert_eq!(db_conversation.num_unread, 0);
            assert_eq!(db_conversation.num_messages_ctx, 0);
            assert_eq!(db_conversation.num_attachments, 0);
            assert_eq!(db_conversation.size, 0);
            assert_eq!(db_conversation.time, 0);
            assert_eq!(db_conversation.time, 0);
            assert_eq!(db_conversation.expiration_time, 0);
            assert_eq!(db_conversation.snooze_time, 0);

            // Check conversation counts have the new conversation.
            {
                let conv_counts = conv_counts_as_map(tx);
                {
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    // unread is 0 due to lack of messages.
                    assert_eq!(label_counts.unread, 0);
                    assert_eq!(label_counts.total, 1);
                }
            }
        });
    });
}

#[test]
fn test_conversation_double_label_without_message_metadata() {
    // Label a conversation with a label that was never assigned without having any message metadata
    // present 2 times and check the data is not duplicated.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_label_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state_and_skip(tx, state.clone(), true);

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");

            let db_conversation = tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .unwrap();

            // Because we have no message metadata, all these values should be empty
            assert_eq!(db_conversation.num_unread, 0);
            assert_eq!(db_conversation.num_messages_ctx, 0);
            assert_eq!(db_conversation.num_attachments, 0);
            assert_eq!(db_conversation.size, 0);
            assert_eq!(db_conversation.time, 0);
            assert_eq!(db_conversation.expiration_time, 0);
            assert_eq!(db_conversation.snooze_time, 0);

            // Check conversation counts have the new conversation.
            {
                let conv_counts = conv_counts_as_map(tx);
                {
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    // unread is 0 due to lack of messages.
                    assert_eq!(label_counts.unread, 0);
                    assert_eq!(label_counts.total, 1);
                }
            }
        });
    });
}

#[test]
fn test_conversation_label_without_metadata_uses_information_from_other_labels() {
    // Check that when we label a conversation without message metadata, we
    // grab the maximum value of the other labels this conversation belongs to.
    // There is a fallback to 0 values if no such thing exists. In production
    // conversation will always be assigned to the "All Mail".
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_label_db_state_label_with_existing_labels();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state_and_skip(tx, state.clone(), true);

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");

            let db_conversation = tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .unwrap();

            // Because we have no message metadata, all these values should be empty
            let conv_label = &state.conversations[0].labels[0];
            assert_eq!(db_conversation.num_unread, conv_label.context_num_unread);
            assert_eq!(
                db_conversation.num_messages_ctx,
                conv_label.context_num_messages
            );
            assert_eq!(
                db_conversation.num_attachments,
                conv_label.context_num_attachments
            );
            assert_eq!(db_conversation.size, conv_label.context_size);
            assert_eq!(db_conversation.time, conv_label.context_time);
            assert_eq!(
                db_conversation.expiration_time,
                conv_label.context_expiration_time
            );
            assert_eq!(db_conversation.snooze_time, conv_label.context_snooze_time);

            // Check conversation counts have the new conversation.
            {
                let conv_counts = conv_counts_as_map(tx);
                {
                    let label_counts = conv_counts.get(&local_label_id1).unwrap();
                    // unread is 0 due to lack of messages.
                    assert_eq!(label_counts.unread, 0);
                    assert_eq!(label_counts.total, 1);
                }
            }
        });
    });
}

#[test]
fn test_conversation_unlabel_with_message_metadata() {
    // Label conversation with a label that was never assigned to the conversation.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_label_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");
            tx.unlabel_conversation(local_label_id1, local_conv_id)
                .expect("failed to unlabel");

            assert!(tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .is_none());

            // Check conversation counts should be 0
            {
                let conv_counts = conv_counts_as_map(tx);
                let label_counts = conv_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 0);
                assert_eq!(label_counts.total, 0);
            }

            // Check message counts should be 0
            {
                let message_counts = msg_counts_as_map(tx);
                let label_counts = message_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 0);
                assert_eq!(label_counts.total, 0);
            }
        });
    });
}

#[test]
fn test_conversation_unlabel_without_message_metadata() {
    // Label and then unlabel a conversation with a label that was never assigned without having any message metadata
    // present.
    with_file_sqlite_db(|mut core_conn, mut conn, _| {
        let state = new_test_label_db_state();
        with_tx_core(&mut core_conn, |core_tx| {
            prepare_db_state_core(core_tx, &state.addresses)
        });
        with_tx(&mut conn, |tx| {
            let (state, state_map) = prepare_and_patch_db_state_and_skip(tx, state.clone(), true);

            let local_conv_id = *state_map
                .conversations
                .get(&state.conversations[0].id)
                .unwrap();
            let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
            tx.label_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");
            tx.unlabel_conversation(local_label_id1, local_conv_id)
                .expect("failed to label");

            assert!(tx
                .get_conversation_with_context(local_conv_id, local_label_id1)
                .expect("failed to get conversation")
                .is_none());

            // Check conversation counts should be 0
            {
                let conv_counts = conv_counts_as_map(tx);
                let label_counts = conv_counts.get(&local_label_id1).unwrap();
                assert_eq!(label_counts.unread, 0);
                assert_eq!(label_counts.total, 0);
            }
        });
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
pub(in crate::db::conversations) fn create_labels(
    tx: &mut MailSqliteConnectionMut,
) -> Vec<LocalLabelId> {
    let labels = [test_label1(), test_label2()];
    tx.create_remote_labels(labels.iter())
        .expect("failed to create labels");

    let r = tx
        .resolve_remote_label_ids(labels.iter().map(|l| &l.id))
        .expect("failed to resolve label ids");
    assert_eq!(r.len(), 2);
    r
}

pub(in crate::db::conversations) fn create_address(core_tx: &mut CoreSqliteConnectionMut) {
    core_tx
        .create_or_update_address(&test_address())
        .expect("failed to create address");
}

pub(in crate::db::conversations) fn test_address() -> Address {
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

pub(in crate::db::conversations) fn test_label1() -> Label {
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

pub(in crate::db::conversations) fn test_label2() -> Label {
    Label {
        id: MY_LABEL_ID2.clone(),
        parent_id: None,
        name: "MyFolder".to_string(),
        path: None,
        color: "#0000".to_string(),
        label_type: LabelType::Folder,
        notify: true,
        display: Default::default(),
        sticky: Default::default(),
        expanded: true,
        order: 1,
    }
}

pub(in crate::db::conversations) fn test_starred_label() -> Label {
    Label {
        id: LabelId::starred().clone(),
        parent_id: None,
        name: "Starred".to_string(),
        path: Some("Starred".to_string()),
        color: "#0000".to_string(),
        label_type: LabelType::System,
        notify: false,
        display: Default::default(),
        sticky: Default::default(),
        expanded: false,
        order: 2,
    }
}

pub(in crate::db::conversations) fn test_conversation(
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
