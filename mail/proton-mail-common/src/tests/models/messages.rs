#![allow(non_snake_case)]

use super::super::*;
use crate::datatypes::{
    AttachmentMetadata, ExclusiveLocation, MessageCount, MessageFlags, SystemLabelId,
};
use crate::db::new_test_connection_file;
use crate::tests::common::{
    create_address, create_labels, test_conversation, test_starred_label, MY_ADDRESS_ID,
    MY_CONVERSATION_ID, MY_LABEL_ID1, MY_LABEL_ID2,
};
use crate::tests::db_states::new_test_delete_db_state;
use crate::tests::utils::{
    conv_counts_as_map, find_conversation_label, msg_counts_as_map, prepare_and_patch_db_state,
    prepare_db_state_core,
};
use lazy_static::lazy_static;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_mail::services::proton::response_data::MessageMetadata as ApiMessageMetadata;
use proton_api_mail::services::proton::response_data::{
    AttachmentMetadata as ApiAttachmentMetadata, ConversationLabel as ApiConversationLabel,
    Disposition as ApiDisposition, Message as ApiMessage, MessageAddress as ApiMessageAddress,
    MessageAttachment as ApiMessageAttachment,
    MessageAttachmentHeaders as ApiMessageAttachmentHeaders, MessageFlags as ApiMessageFlags,
    MimeType as ApiMimeType,
};
use proton_core_common::datatypes::{LabelId, RemoteId};
use proton_crypto_inbox::attachment::KeyPackets;
use stash::orm::Model;
use stash::stash::{StashError, Tether};
use velcro::hash_map;

#[tokio::test]
async fn test_create_message() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    test_create_message_dependencies_core(&tx).await;
    let _conv_id = test_create_message_dependencies(&tx).await;
    let message =
        test_message_with_metadata(vec![LabelId::inbox().into(), MY_LABEL_ID1.clone()], vec![]);
    let id = Message::create_or_update_messages_from_metadata(
        vec![message.metadata.clone()],
        tx.stash(),
    )
    .await
    .expect("failed to create message")
    .into_iter()
    .next()
    .unwrap();
    let db_message = Message::load(id, tx.stash())
        .await
        .expect("failed to get message")
        .expect("must have a value");
    let mut expected = Message::from(message);
    expected.set_stash(&stash);
    expected.local_id = Some(1);
    expected.row_id = Some(1);
    expected.exclusive_location = Some(ExclusiveLocation::Inbox);

    assert_eq!(db_message, expected);
    assert_eq!(db_message.label_ids.len(), 2);
}

// #[test]
// fn test_create_message_without_synced_conversation() {
//     // Validate that we can create messages without having fetch the conversation.
//     with_file_sqlite_db(|mut core_conn, mut conn, _| {
//         with_tx_core(&mut core_conn, test_create_message_dependencies_core);
//         with_tx(&mut conn, |tx| {
//             create_labels(tx);
//             let metadata = test_message_metadata([MY_LABEL_ID1.clone()], []);
//             let id = tx
//                 .create_message_from_metadata(&metadata)
//                 .expect("failed to create message");
//             let db_metadata = tx
//                 .get_message_metadata(id)
//                 .expect("failed to get message")
//                 .expect("must have a value");
//
//             // ensure we can't access this conversation
//             let conv = tx.get_conversation(db_metadata.conversation_id).unwrap();
//             assert!(conv.is_none());
//             let (is_known, rid) = tx
//                 .is_conversation_known(db_metadata.conversation_id)
//                 .unwrap();
//             assert!(!is_known);
//             assert_eq!(rid, Some(metadata.conversation_id.clone()));
//
//             // create the conversation
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
//                 [],
//             );
//             tx.create_conversation(&conversation)
//                 .expect("failed to create conversation");
//
//             let conv = tx.get_conversation(db_metadata.conversation_id).unwrap();
//             assert!(conv.is_some());
//             let (is_known, rid) = tx
//                 .is_conversation_known(db_metadata.conversation_id)
//                 .unwrap();
//             assert!(is_known);
//             assert_eq!(rid, Some(metadata.conversation_id.clone()));
//         });
//     });
// }

#[tokio::test]
async fn test_create_message_with_attachments() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    test_create_message_dependencies_core(&tx).await;
    let attachment_metadata = ApiAttachmentMetadata {
        id: ApiRemoteId::from("myattachment"),
        size: 80,
        name: "foo.pdf".to_owned(),
        mime_type: ApiMimeType::ApplicationPdf,
        disposition: ApiDisposition::Inline,
    };
    let _ = test_create_message_dependencies(&tx).await;
    let message = test_message_with_metadata(
        vec![MY_LABEL_ID1.clone()],
        vec![attachment_metadata.clone()],
    );
    let id = Message::create_or_update_messages_from_metadata(vec![message.metadata], tx.stash())
        .await
        .expect("failed to create message")
        .into_iter()
        .next()
        .unwrap();

    let db_message = Message::load(id, tx.stash())
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
//                 mime_type: MimeType::TextPlain,
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
    let tx = stash.connection();
    stash.execute("DELETE FROM labels", vec![]).await.unwrap();
    test_create_message_dependencies_core(&tx).await;
    let _conv_id = test_create_message_dependencies(&tx).await;
    test_starred_label().save_using(&tx).await.unwrap();
    let message = test_message_with_metadata(vec![MY_LABEL_ID1.clone()], vec![]);
    let mut metadata_updated = test_message_with_metadata(
        vec![MY_LABEL_ID2.clone(), LabelId::starred().clone().into()],
        vec![],
    );
    metadata_updated.metadata.order = 20;
    metadata_updated.metadata.unread = true;
    metadata_updated
        .metadata
        .label_ids
        .push(LabelId::starred().clone().into());
    // This value contains unused flags.
    metadata_updated.metadata.flags = ApiMessageFlags::from_bits(8397841).unwrap();
    let id = Message::create_or_update_messages_from_metadata(vec![message.metadata], tx.stash())
        .await
        .expect("failed to create message")
        .into_iter()
        .next()
        .unwrap();

    let mut db_message = Message::load(id, tx.stash())
        .await
        .expect("failed to get message")
        .expect("must have a value");
    db_message.display_order = metadata_updated.metadata.order;
    db_message.unread = metadata_updated.metadata.unread;
    db_message.label_ids = metadata_updated
        .metadata
        .label_ids
        .iter()
        .map(|l| l.clone().into())
        .collect();
    db_message.flags = MessageFlags::from(metadata_updated.metadata.flags);
    db_message.save().await.expect("failed to update message");
    let mut expected = Message::from(metadata_updated);
    expected.set_stash(&stash);
    expected.local_id = Some(1);
    expected.row_id = Some(1);
    assert_eq!(db_message, expected);
    assert!(db_message.is_starred());
    assert_eq!(db_message.label_ids.len(), 3);
    let db_message = Message::load(id, tx.stash())
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
    let tx = stash.connection();
    create_address(&tx).await;
    let labels = create_labels(&tx).await;
    let counts = vec![
        MessageCount {
            label_id: MY_LABEL_ID1.clone().into(),
            total: 20,
            unread: 4,
        },
        MessageCount {
            label_id: MY_LABEL_ID2.clone().into(),
            total: 400,
            unread: 124,
        },
    ];

    Label::create_or_update_message_counts(counts.clone(), tx.stash())
        .await
        .expect("failed to creat counters");
    let db_labels = Label::all(tx.stash(), None)
        .await
        .expect("failed to get counters");
    let db_counters = db_labels
        .iter()
        .map(|c| MessageCount {
            label_id: c.remote_id.clone().unwrap(),
            total: c.total_msg,
            unread: c.unread_msg,
        })
        .collect::<Vec<_>>();
    assert!(db_counters.contains(&counts[0]));
    assert!(db_counters.contains(&counts[1]));

    let label_msg_count = Label::load(labels[0], tx.stash()).await.unwrap().unwrap();
    assert!(db_labels.contains(&label_msg_count));

    assert_eq!(db_labels.len(), 1);
    assert_eq!(db_labels[0].remote_id, counts[0].label_id.clone().into());
    assert_eq!(db_labels[0].total_msg, counts[0].total);
    assert_eq!(db_labels[0].unread_msg, counts[0].unread);
}

#[tokio::test]
#[ignore]
pub async fn test_delete_local_message() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    // Deleting a message must
    // * Update conversation counters
    // * Update conversation labels
    // * Update message counters
    let (mut state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(state.conversations[0].remote_id.as_ref().unwrap())
        .unwrap();
    {
        // Delete 3rd message from 1st conversation.
        let message = &mut state.messages[2];
        let _local_id = *state_map
            .messages
            .get(&message.remote_id.clone().unwrap())
            .unwrap();

        message.deleted = true;
        message
            .save_using(&tx)
            .await
            .expect("failed to mark local message as deleted");

        let conv_counts = conv_counts_as_map(&tx).await;
        let msg_counts = msg_counts_as_map(&tx).await;

        for label in &mut message.label_ids {
            let local_label_id = *state_map
                .labels
                .get(label)
                .expect("Failed to resolve label");
            let conv_count = conv_counts.get(&local_label_id).unwrap();
            let start_conv_count = state_map.conversation_counts.get(label).unwrap();
            let start_msg_count = state_map.message_counts.get(label).unwrap();

            let local_conv = Conversation::load(local_conv_id, tx.stash())
                .await
                .unwrap()
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
            assert_eq!(
                local_conv.num_messages,
                state.conversations[0].num_messages - 1
            );

            let local_conv = Conversation::load(local_conv_id, tx.stash())
                .await
                .unwrap()
                .unwrap();

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
            .filter(|m| m.remote_conversation_id == state.conversations[0].remote_id)
            .map(|m| {
                *state_map
                    .messages
                    .get(&m.remote_id.clone().unwrap())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for id in &ids {
            let mut message = Message::load(*id, tx.stash())
                .await
                .expect("failed to get message")
                .expect("must have a value");
            message.deleted = true;
            message
                .save_using(&tx)
                .await
                .expect("failed to mark local message as deleted");
        }

        let conv_counts = conv_counts_as_map(&tx).await;
        let msg_counts = msg_counts_as_map(&tx).await;

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
            if label.remote_label_id == state.labels[0].remote_id {
                assert_eq!(msg_count.total, start_msg_count.total - 3);
            } else {
                assert_eq!(msg_count.total, start_msg_count.total - 1);
            }
        }

        // Conversation should be deleted
        assert!(Conversation::load(local_conv_id, tx.stash())
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
#[ignore]
pub async fn test_message_metadata_list() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (_, _state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;
    let messages = Message::all(tx.stash(), None)
        .await
        .expect("failed to get messages");
    assert_eq!(messages.len(), 3);
    assert!(messages[0].time > messages[1].time);
    assert!(messages[1].time > messages[2].time);
}

#[tokio::test]
#[ignore]
pub async fn test_delete_local_message_does_not_change_conv_unread_count() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    let (mut state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    // Delete 2nd message from 1st conversation.
    let message = &mut state.messages[0];
    let _local_id = *state_map
        .messages
        .get(&message.remote_id.clone().unwrap())
        .unwrap();
    message.deleted = true;
    message
        .save_using(&tx)
        .await
        .expect("failed to mark local message as deleted");
    let local_label_id = state_map.labels.get(&MY_LABEL_ID1.clone().into()).unwrap();

    let conv_counts = conv_counts_as_map(&tx).await;
    let label_conv_counts = conv_counts.get(local_label_id).unwrap();
    assert_eq!(label_conv_counts.unread, 1);
}

#[tokio::test]
pub async fn test_undelete_local_message() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    let mut state = new_test_delete_db_state();
    prepare_db_state_core(&tx, &mut state.addresses).await;
    // Same as test_delete_local_message, but undo the operations
    let (mut state, state_map) = prepare_and_patch_db_state(&tx, state.clone()).await;

    let local_conv_id = *state_map
        .conversations
        .get(&state.conversations[0].remote_id.clone().unwrap())
        .unwrap();
    {
        // Delete 3rd message from 1st conversation.
        let message = &mut state.messages[2];
        let _local_id = *state_map
            .messages
            .get(&message.remote_id.clone().unwrap())
            .unwrap();
        message.deleted = true;
        message
            .save_using(&tx)
            .await
            .expect("failed to mark local message as deleted");

        message.deleted = false;
        message
            .save_using(&tx)
            .await
            .expect("failed to undelete message");

        let conv_counts = conv_counts_as_map(&tx).await;
        let msg_counts = msg_counts_as_map(&tx).await;

        for label in &mut message.label_ids {
            let local_label_id = *state_map
                .labels
                .get(label)
                .expect("Failed to resolve label");
            let conv_count = conv_counts.get(&local_label_id).unwrap();
            let start_conv_count = state_map.conversation_counts.get(label).unwrap();
            let start_msg_count = state_map.message_counts.get(label).unwrap();

            let local_conv = Conversation::load(local_conv_id, tx.stash())
                .await
                .unwrap()
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
            assert_eq!(local_conv.num_messages, state.conversations[0].num_messages,);

            let local_conv = Conversation::load(local_conv_id, tx.stash())
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
        for id in &ids {
            let mut message = Message::load(*id, tx.stash())
                .await
                .expect("failed to get message")
                .expect("must have a value");
            message.deleted = true;
            message
                .save_using(&tx)
                .await
                .expect("failed to mark local message as deleted");
        }
        for id in &ids {
            let mut message = Message::load(*id, tx.stash())
                .await
                .expect("failed to get message")
                .expect("must have a value");
            message.deleted = false;
            message
                .save_using(&tx)
                .await
                .expect("failed to mark local message as deleted");
        }

        let conv_counts = conv_counts_as_map(&tx).await;
        let msg_counts = msg_counts_as_map(&tx).await;

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
        assert!(Conversation::load(local_conv_id, tx.stash())
            .await
            .unwrap()
            .is_some());
    }
}

#[tokio::test]
async fn test_create_message_and_body() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    test_create_message_dependencies_core(&tx).await;
    test_create_message_dependencies(&tx).await;
    let message = ApiMessage {
        metadata: test_message_metadata(vec![MY_LABEL_ID1.clone()], vec![]),
        header: "my headers".to_owned(),
        parsed_headers: hash_map! {
            "foo".to_owned(): "bar".to_owned(),
            "zeta".to_owned(): "gama".to_owned(),
        },
        body: "my_message".to_owned(),
        mime_type: ApiMimeType::TextPlain,
        attachments: vec![],
    };
    let id = Message::create_or_update_messages_from_metadata(vec![message.metadata], tx.stash())
        .await
        .expect("failed to create message")
        .into_iter()
        .next()
        .unwrap();
    let db_message = Message::load(id, tx.stash())
        .await
        .expect("failed to get message")
        .expect("must have a value");
    let mut metadata = MessageBodyMetadata {
        local_message_id: None,
        remote_message_id: db_message.remote_id.clone(),
        header: db_message.header.clone(),
        parsed_headers: db_message.parsed_headers.clone(),
        mime_type: db_message.mime_type,
        row_id: None,
        stash: Some(stash.clone()),
    };
    metadata
        .save()
        .await
        .expect("failed to store message body metadata in db");

    assert_eq!(id, metadata.local_message_id.unwrap());

    let db_message_body = MessageBodyMetadata::load(id, tx.stash())
        .await
        .expect("failed to get message body")
        .expect("must have a value");

    assert_eq!(metadata, db_message_body);

    let expected = MessageBodyMetadata {
        local_message_id: db_message.local_id,
        remote_message_id: db_message.remote_id.clone(),
        header: db_message.header.clone(),
        parsed_headers: db_message.parsed_headers.clone(),
        mime_type: db_message.mime_type,
        row_id: Some(1),
        stash: Some(stash.clone()),
    };

    assert_eq!(db_message_body, expected);
}

#[tokio::test]
async fn test_update_message_and_body() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    test_create_message_dependencies_core(&tx).await;
    test_create_message_dependencies(&tx).await;
    let mut message = ApiMessage {
        metadata: test_message_metadata(vec![MY_LABEL_ID1.clone()], vec![]),
        header: "my headers".to_owned(),
        parsed_headers: hash_map! {
            "foo".to_owned(): "bar".to_owned(),
            "zeta".to_owned(): "gama".to_owned(),
        },
        body: "my_message".to_owned(),
        mime_type: ApiMimeType::TextPlain,
        attachments: vec![],
    };
    let id = Message::create_or_update_messages_from_metadata(vec![message.metadata], tx.stash())
        .await
        .expect("failed to create message")
        .into_iter()
        .next()
        .unwrap();

    let db_message = Message::load(id, tx.stash())
        .await
        .expect("failed to get message")
        .expect("must have a value");
    let mut metadata = MessageBodyMetadata {
        local_message_id: None,
        remote_message_id: db_message.remote_id.clone(),
        header: db_message.header.clone(),
        parsed_headers: db_message.parsed_headers.clone(),
        mime_type: db_message.mime_type,
        row_id: None,
        stash: Some(stash.clone()),
    };
    metadata
        .save()
        .await
        .expect("failed to store message body metadata in db");

    // Update the body
    message
        .parsed_headers
        .insert("marco".to_owned(), "polo".to_owned());
    message.header = "new header".to_owned();
    message.body = "new body type".to_owned();
    message.mime_type = ApiMimeType::TextHtml;

    let db_message_body = MessageBodyMetadata::load(id, tx.stash())
        .await
        .expect("failed to get message body")
        .expect("must have a value");

    let expected = MessageBodyMetadata {
        local_message_id: db_message.local_id,
        remote_message_id: db_message.remote_id.clone(),
        header: db_message.header.clone(),
        parsed_headers: db_message.parsed_headers.clone(),
        mime_type: db_message.mime_type,
        row_id: Some(1),
        stash: Some(stash.clone()),
    };

    assert_eq!(db_message_body, expected);
}

#[tokio::test]
async fn test_create_message_and_body_with_attachments() {
    let (stash, _db_dir) = new_test_connection_file().await;
    let tx = stash.connection();
    test_create_message_dependencies_core(&tx).await;
    let attachment_id = RemoteId::from("attachment");
    test_create_message_dependencies(&tx).await;
    let message = ApiMessage {
        metadata: test_message_metadata(
            vec![MY_LABEL_ID1.clone()],
            vec![ApiAttachmentMetadata {
                id: attachment_id.clone().into(),
                size: 1024,
                name: "fooo".to_owned(),
                mime_type: ApiMimeType::TextHtml,
                disposition: ApiDisposition::Inline,
            }],
        ),
        header: "my headers".to_owned(),
        parsed_headers: hash_map! {
            "foo".to_owned(): "bar".to_owned(),
            "zeta".to_owned(): "gama".to_owned(),
        },
        body: "my_message".to_owned(),
        mime_type: ApiMimeType::TextPlain,
        attachments: vec![ApiMessageAttachment {
            id: attachment_id.clone().into(),
            name: "fooo".to_owned(),
            size: 1024,
            mime_type: ApiMimeType::TextHtml,
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
        }],
    };
    let id = Message::create_or_update_messages_from_metadata(vec![message.metadata], tx.stash())
        .await
        .expect("failed to create message")
        .into_iter()
        .next()
        .unwrap();

    let db_message = Message::load(id, tx.stash())
        .await
        .expect("failed to get message")
        .expect("must have a value");
    let mut metadata = MessageBodyMetadata {
        local_message_id: db_message.local_id,
        remote_message_id: db_message.remote_id.clone(),
        header: db_message.header.clone(),
        parsed_headers: db_message.parsed_headers.clone(),
        mime_type: db_message.mime_type,
        row_id: db_message.row_id,
        stash: Some(stash.clone()),
    };
    metadata
        .save()
        .await
        .or_else(|err| match err {
            StashError::NoRowsUpdated => Ok(()),
            _ => Err(err),
        })
        .expect("failed to store message body metadata in db");

    let local_attachment = message.attachments.first().unwrap();

    assert_eq!(
        local_attachment.headers.content_id,
        message.attachments[0].headers.content_id
    );
    assert_eq!(
        local_attachment.headers.content_transfer_encoding,
        message.attachments[0].headers.content_transfer_encoding
    );
    assert_eq!(
        local_attachment.headers.image_width,
        message.attachments[0].headers.image_width
    );
    assert_eq!(
        local_attachment.headers.image_height,
        message.attachments[0].headers.image_height
    );
}

// #[test]
// fn messages_mark_read() {
//     // Mark conversation as read and update all conversation / message counts
//     with_file_sqlite_db(|mut core_conn, mut conn, _| {
//         let state = new_test_unread_db_state();
//         with_tx_core(&mut core_conn, |core_tx| {
//             prepare_db_state_core(core_tx, &state.addresses)
//         });
//         with_tx(&mut conn, |tx| {
//             let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());
//
//             let local_conv_id = *state_map
//                 .conversations
//                 .get(&state.conversations[0].id)
//                 .unwrap();
//             let local_msg_id1 = *state_map.messages.get(&state.messages[0].id).unwrap();
//             let local_msg_id3 = *state_map.messages.get(&state.messages[2].id).unwrap();
//             let local_msg_id4 = *state_map.messages.get(&state.messages[3].id).unwrap();
//             let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
//             let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
//
//             let check_counters = |tx: &mut MailSqliteConnectionMut,
//                                   label_1_msg_diff: u64,
//                                   label_1_conv_diff: u64| {
//                 // Check conversation counts
//                 {
//                     let conv_counts = conv_counts_as_map(tx);
//                     // Check conversation label1 values, values should be unchanged.
//                     {
//                         let start_label_counts =
//                             state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
//                         let label_counts = conv_counts.get(&local_label_id1).unwrap();
//                         assert_eq!(
//                             label_counts.unread,
//                             start_label_counts.unread - label_1_conv_diff
//                         );
//                         assert_eq!(label_counts.total, start_label_counts.total);
//                     }
//                     // Check conversation label2 values - should be unchanged.
//                     {
//                         let start_label_counts =
//                             state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
//                         let label_counts = conv_counts.get(&local_label_id2).unwrap();
//                         assert_eq!(label_counts.unread, start_label_counts.unread);
//                         assert_eq!(label_counts.total, start_label_counts.total);
//                     }
//                 }
//
//                 // Check message counts
//                 {
//                     let message_counts = msg_counts_as_map(tx);
//
//                     // Check label1
//                     {
//                         let start_label_counts =
//                             state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
//                         let label_counts = message_counts.get(&local_label_id1).unwrap();
//                         assert_eq!(
//                             label_counts.unread,
//                             start_label_counts.unread - label_1_msg_diff
//                         );
//                         assert_eq!(label_counts.total, start_label_counts.total);
//                     }
//                     // Check label2 - should be unchanged.
//                     {
//                         let start_label_counts =
//                             state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
//                         let label_counts = message_counts.get(&local_label_id2).unwrap();
//                         assert_eq!(label_counts.unread, start_label_counts.unread);
//                         assert_eq!(label_counts.total, start_label_counts.total);
//                     }
//                 }
//             };
//
//             tx.mark_message_read(local_msg_id1)
//                 .expect("failed to mark as read");
//             let db_message = tx
//                 .get_message_metadata(local_msg_id1)
//                 .expect("failed to get message")
//                 .unwrap();
//
//             // Msg is read.
//             assert_eq!(db_message.unread, false);
//
//             let db_conv = tx
//                 .get_conversation_with_context(local_conv_id, local_label_id1)
//                 .unwrap()
//                 .unwrap();
//             assert_eq!(db_conv.num_unread, 2);
//
//             check_counters(tx, 1, 0);
//             tx.mark_message_read(local_msg_id3)
//                 .expect("failed to mark as read");
//             check_counters(tx, 2, 0);
//             tx.mark_message_read(local_msg_id4)
//                 .expect("failed to mark as read");
//             // All conversation messages on label_1 have been marked as read, we should now see an
//             // updated
//             // conversation count.
//             check_counters(tx, 3, 1);
//
//             let db_conv = tx
//                 .get_conversation_with_context(local_conv_id, local_label_id1)
//                 .unwrap()
//                 .unwrap();
//             assert_eq!(db_conv.num_unread, 0);
//         });
//     });
// }

// #[test]
// fn messages_mark_unread() {
//     // Mark conversation as read and update all conversation / message counts
//     with_file_sqlite_db(|mut core_conn, mut conn, _| {
//         let state = new_test_unread_db_state();
//         with_tx_core(&mut core_conn, |core_tx| {
//             prepare_db_state_core(core_tx, &state.addresses)
//         });
//         with_tx(&mut conn, |tx| {
//             let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());
//
//             let local_conv_id = *state_map
//                 .conversations
//                 .get(&state.conversations[0].id)
//                 .unwrap();
//
//             let local_msg_id1 = *state_map.messages.get(&state.messages[0].id).unwrap();
//             let local_msg_id3 = *state_map.messages.get(&state.messages[2].id).unwrap();
//             let local_msg_id4 = *state_map.messages.get(&state.messages[3].id).unwrap();
//             let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
//             let local_label_id2 = *state_map.labels.get(&MY_LABEL_ID2).unwrap();
//
//             // mark messages read (also servers as bulk test).
//             tx.mark_messages_read([local_msg_id1, local_msg_id3, local_msg_id4])
//                 .expect("failed to mark as read");
//
//             let check_counters = |tx: &mut MailSqliteConnectionMut,
//                                   label_1_msg_diff: u64,
//                                   label_1_conv_diff: u64| {
//                 // Check conversation counts
//                 {
//                     let conv_counts = conv_counts_as_map(tx);
//                     // Check conversation label1 values, values should be unchanged.
//                     {
//                         let start_label_counts =
//                             state_map.conversation_counts.get(&MY_LABEL_ID1).unwrap();
//                         let label_counts = conv_counts.get(&local_label_id1).unwrap();
//                         assert_eq!(
//                             label_counts.unread,
//                             start_label_counts.unread - label_1_conv_diff
//                         );
//                         assert_eq!(label_counts.total, start_label_counts.total);
//                     }
//                     // Check conversation label2 values - should be unchanged.
//                     {
//                         let start_label_counts =
//                             state_map.conversation_counts.get(&MY_LABEL_ID2).unwrap();
//                         let label_counts = conv_counts.get(&local_label_id2).unwrap();
//                         assert_eq!(label_counts.unread, start_label_counts.unread);
//                         assert_eq!(label_counts.total, start_label_counts.total);
//                     }
//                 }
//
//                 // Check message counts
//                 {
//                     let message_counts = msg_counts_as_map(tx);
//
//                     // Check label1
//                     {
//                         let start_label_counts =
//                             state_map.message_counts.get(&MY_LABEL_ID1).unwrap();
//                         let label_counts = message_counts.get(&local_label_id1).unwrap();
//                         assert_eq!(
//                             label_counts.unread,
//                             start_label_counts.unread - label_1_msg_diff
//                         );
//                         assert_eq!(label_counts.total, start_label_counts.total);
//                     }
//                     // Check label2 - should be unchanged.
//                     {
//                         let start_label_counts =
//                             state_map.message_counts.get(&MY_LABEL_ID2).unwrap();
//                         let label_counts = message_counts.get(&local_label_id2).unwrap();
//                         assert_eq!(label_counts.unread, start_label_counts.unread);
//                         assert_eq!(label_counts.total, start_label_counts.total);
//                     }
//                 }
//             };
//
//             check_counters(tx, 3, 1);
//
//             tx.mark_message_unread(local_msg_id1)
//                 .expect("failed to mark as read");
//             let db_message = tx
//                 .get_message_metadata(local_msg_id1)
//                 .expect("failed to get message")
//                 .unwrap();
//             // Msg is unread.
//             assert_eq!(db_message.unread, true);
//
//             let db_conv = tx
//                 .get_conversation_with_context(local_conv_id, local_label_id1)
//                 .unwrap()
//                 .unwrap();
//             assert_eq!(db_conv.num_unread, 1);
//
//             check_counters(tx, 2, 0);
//             tx.mark_message_unread(local_msg_id3)
//                 .expect("failed to mark as read");
//             check_counters(tx, 1, 0);
//             tx.mark_message_unread(local_msg_id4)
//                 .expect("failed to mark as read");
//             // All conversation messages on label_1 have been marked as read, we should now see an
//             // updated
//             // conversation count.
//             check_counters(tx, 0, 0);
//
//             let db_conv = tx
//                 .get_conversation_with_context(local_conv_id, local_label_id1)
//                 .unwrap()
//                 .unwrap();
//             assert_eq!(db_conv.num_unread, 3);
//         });
//     });
// }

// #[test]
// fn label_messages() {
//     // Label conversation with a label that was never assigned to the conversation.
//     with_file_sqlite_db(|mut core_conn, mut conn, _| {
//         let state = new_test_label_db_state();
//         with_tx_core(&mut core_conn, |core_tx| {
//             prepare_db_state_core(core_tx, &state.addresses)
//         });
//         with_tx(&mut conn, |tx| {
//             let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());
//
//             let local_conv_id = *state_map
//                 .conversations
//                 .get(&state.conversations[0].id)
//                 .unwrap();
//             let local_msg_id1 = *state_map.messages.get(&state.messages[0].id).unwrap();
//             let local_msg_id2 = *state_map.messages.get(&state.messages[1].id).unwrap();
//             let local_msg_id3 = *state_map.messages.get(&state.messages[2].id).unwrap();
//             let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
//             tx.label_message(local_label_id1, local_msg_id1)
//                 .expect("failed to label");
//
//             let db_conversation = tx
//                 .get_conversation_with_context(local_conv_id, local_label_id1)
//                 .expect("failed to get conversation")
//                 .unwrap();
//
//             // There should be 1 unread message.
//             assert_eq!(db_conversation.num_unread, 0);
//             assert_eq!(db_conversation.num_messages_ctx, 1);
//             assert_eq!(db_conversation.num_attachments, 1);
//             assert_eq!(db_conversation.size, state.messages[0].size,);
//             assert_eq!(db_conversation.time, state.messages[0].time,);
//             assert_eq!(
//                 db_conversation.expiration_time,
//                 state.messages[0].expiration_time,
//             );
//             assert_eq!(db_conversation.snooze_time, state.messages[0].snooze_time,);
//
//             // Check conversation counts have the new conversation.
//             {
//                 let conv_counts = conv_counts_as_map(tx);
//                 let label_counts = conv_counts.get(&local_label_id1).unwrap();
//                 assert_eq!(label_counts.unread, 0);
//                 assert_eq!(label_counts.total, 1);
//             }
//
//             // Check message counts.
//             {
//                 let message_counts = msg_counts_as_map(tx);
//                 let label_counts = message_counts.get(&local_label_id1).unwrap();
//                 assert_eq!(label_counts.unread, 0);
//                 assert_eq!(label_counts.total, 1);
//             }
//
//             let check_full_conversations = |tx: &mut MailSqliteConnectionMut| {
//                 // Check conversation after all messages have been labeled.
//                 let db_conversation = tx
//                     .get_conversation_with_context(local_conv_id, local_label_id1)
//                     .expect("failed to get conversation")
//                     .unwrap();
//                 assert_eq!(db_conversation.num_unread, 1);
//                 assert_eq!(db_conversation.num_messages_ctx, 3);
//                 assert_eq!(db_conversation.num_attachments, 1);
//                 assert_eq!(
//                     db_conversation.size,
//                     state.messages.iter().fold(0, |x, m| x + m.size)
//                 );
//                 assert_eq!(
//                     db_conversation.time,
//                     state.messages.iter().fold(0, |x, m| x.max(m.time))
//                 );
//                 assert_eq!(
//                     db_conversation.expiration_time,
//                     state
//                         .messages
//                         .iter()
//                         .fold(0, |x, m| x.max(m.expiration_time))
//                 );
//                 assert_eq!(
//                     db_conversation.snooze_time,
//                     state.messages.iter().fold(0, |x, m| x.max(m.snooze_time))
//                 );
//
//                 // Check conversation counts.
//                 {
//                     let conv_counts = conv_counts_as_map(tx);
//                     let label_counts = conv_counts.get(&local_label_id1).unwrap();
//                     assert_eq!(label_counts.unread, 1);
//                     assert_eq!(label_counts.total, 1);
//                 }
//
//                 // Check message counts.
//                 {
//                     let message_counts = msg_counts_as_map(tx);
//                     let label_counts = message_counts.get(&local_label_id1).unwrap();
//                     assert_eq!(label_counts.unread, 1);
//                     assert_eq!(label_counts.total, 3);
//                 }
//             };
//
//             // Label remaining messages.
//             tx.label_messages(local_label_id1, [local_msg_id2, local_msg_id3])
//                 .unwrap();
//
//             check_full_conversations(tx);
//
//             // Apply again, should be noop.
//             tx.label_messages(
//                 local_label_id1,
//                 [local_msg_id1, local_msg_id2, local_msg_id3],
//             )
//             .unwrap();
//
//             check_full_conversations(tx);
//         });
//     });
// }

// #[test]
// fn unlabel_messages() {
//     // Label conversation with a label that was never assigned to the conversation.
//     with_file_sqlite_db(|mut core_conn, mut conn, _| {
//         let state = new_test_label_db_state();
//         with_tx_core(&mut core_conn, |core_tx| {
//             prepare_db_state_core(core_tx, &state.addresses)
//         });
//         with_tx(&mut conn, |tx| {
//             let (state, state_map) = prepare_and_patch_db_state(tx, state.clone());
//
//             let local_conv_id = *state_map
//                 .conversations
//                 .get(&state.conversations[0].id)
//                 .unwrap();
//             let local_msg_id1 = *state_map.messages.get(&state.messages[0].id).unwrap();
//             let local_msg_id2 = *state_map.messages.get(&state.messages[1].id).unwrap();
//             let local_msg_id3 = *state_map.messages.get(&state.messages[2].id).unwrap();
//             let local_label_id1 = *state_map.labels.get(&MY_LABEL_ID1).unwrap();
//             tx.label_messages(
//                 local_label_id1,
//                 [local_msg_id1, local_msg_id2, local_msg_id3],
//             )
//             .expect("failed to label");
//
//             // unlabel first message.
//             tx.unlabel_message(local_label_id1, local_msg_id1).unwrap();
//
//             let remote_msg_id1 = state.messages[0].id.clone();
//
//             let db_conversation = tx
//                 .get_conversation_with_context(local_conv_id, local_label_id1)
//                 .expect("failed to get conversation")
//                 .unwrap();
//
//             // Check conversation status.
//             assert_eq!(db_conversation.num_unread, 1);
//             assert_eq!(db_conversation.num_messages_ctx, 2);
//             assert_eq!(db_conversation.num_attachments, 0);
//             assert_eq!(
//                 db_conversation.size,
//                 state
//                     .messages
//                     .iter()
//                     .filter(|m| m.id != remote_msg_id1)
//                     .fold(0, |x, m| x + m.size)
//             );
//             assert_eq!(
//                 db_conversation.time,
//                 state
//                     .messages
//                     .iter()
//                     .filter(|m| m.id != remote_msg_id1)
//                     .fold(0, |x, m| x.max(m.time))
//             );
//             assert_eq!(
//                 db_conversation.expiration_time,
//                 state
//                     .messages
//                     .iter()
//                     .filter(|m| m.id != remote_msg_id1)
//                     .fold(0, |x, m| x.max(m.expiration_time))
//             );
//             assert_eq!(
//                 db_conversation.snooze_time,
//                 state
//                     .messages
//                     .iter()
//                     .filter(|m| m.id != remote_msg_id1)
//                     .fold(0, |x, m| x.max(m.snooze_time))
//             );
//
//             // Check conversation counts have the new conversation.
//             {
//                 let conv_counts = conv_counts_as_map(tx);
//                 let label_counts = conv_counts.get(&local_label_id1).unwrap();
//                 assert_eq!(label_counts.unread, 1);
//                 assert_eq!(label_counts.total, 1);
//             }
//
//             // Check message counts.
//             {
//                 let message_counts = msg_counts_as_map(tx);
//                 let label_counts = message_counts.get(&local_label_id1).unwrap();
//                 assert_eq!(label_counts.unread, 1);
//                 assert_eq!(label_counts.total, 2);
//             }
//
//             let check_final_conv_state = |tx: &mut MailSqliteConnectionMut| {
//                 // Check conversation after all messages have been labeled.
//                 assert!(tx
//                     .get_conversation_with_context(local_conv_id, local_label_id1)
//                     .expect("failed to get conversation")
//                     .is_none());
//
//                 // Check conversation counts.
//                 {
//                     let conv_counts = conv_counts_as_map(tx);
//                     let label_counts = conv_counts.get(&local_label_id1).unwrap();
//                     assert_eq!(label_counts.unread, 0);
//                     assert_eq!(label_counts.total, 0);
//                 }
//
//                 // Check message counts.
//                 {
//                     let message_counts = msg_counts_as_map(tx);
//                     let label_counts = message_counts.get(&local_label_id1).unwrap();
//                     assert_eq!(label_counts.unread, 0);
//                     assert_eq!(label_counts.total, 0);
//                 }
//             };
//
//             // Label remaining messages.
//             tx.unlabel_messages(local_label_id1, [local_msg_id2, local_msg_id3])
//                 .unwrap();
//
//             check_final_conv_state(tx);
//
//             // Apply again, should be noop.
//             tx.unlabel_messages(
//                 local_label_id1,
//                 [local_msg_id1, local_msg_id2, local_msg_id3],
//             )
//             .unwrap();
//
//             check_final_conv_state(tx);
//         });
//     });
// }

lazy_static! {
    pub(super) static ref MY_MESSAGE_ID: RemoteId = RemoteId::from("MyRemoteId");
}

async fn test_create_message_dependencies_core(tx_core: &Tether) {
    create_address(tx_core).await;
}

async fn test_create_message_dependencies(tx: &Tether) -> u64 {
    create_labels(tx).await;
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
    conversation.set_stash(tx.stash());
    conversation
        .save()
        .await
        .expect("failed to create conversation");
    conversation.local_id.unwrap()
}

fn test_message_metadata(
    label_ids: Vec<ApiRemoteId>,
    attachments: Vec<ApiAttachmentMetadata>,
) -> ApiMessageMetadata {
    ApiMessageMetadata {
        id: MY_MESSAGE_ID.clone().into(),
        conversation_id: MY_CONVERSATION_ID.clone(),
        order: 1,
        address_id: MY_ADDRESS_ID.clone(),
        label_ids: label_ids.into_iter().collect(),
        external_id: None,
        subject: "Hello ".to_owned(),
        sender: ApiMessageAddress {
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
    label_ids: Vec<ApiRemoteId>,
    attachments: Vec<ApiAttachmentMetadata>,
) -> ApiMessage {
    ApiMessage {
        attachments: vec![],
        body: "".to_owned(),
        header: "".to_owned(),
        mime_type: Default::default(),
        parsed_headers: Default::default(),
        metadata: ApiMessageMetadata {
            id: MY_MESSAGE_ID.clone().into(),
            conversation_id: MY_CONVERSATION_ID.clone(),
            order: 1,
            address_id: MY_ADDRESS_ID.clone(),
            label_ids: label_ids.into_iter().collect(),
            external_id: None,
            subject: "Hello ".to_owned(),
            sender: ApiMessageAddress {
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
