use super::*;
use crate::datatypes::DeletedItemType;
use crate::models::{Conversation, DeletedItem, Message};
use crate::test_utils::db::new_test_connection_file;
use crate::test_utils::utils::create_address;
use crate::{conversation, message};
use proton_core_common::datatypes::UnixTimestamp;
use proton_core_common::event_loop::events::Action;
use proton_core_common::models::ModelExtension;
use proton_core_common::models::ModelIdExtension;
use stash::orm::Model;
use stash::stash::StashError;
use test_case::test_case;

#[tokio::test]
async fn test_deleted_item_save() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            let mut item = DeletedItem::new("msg_123".to_string(), DeletedItemType::Message);
            item.save(tx).await.unwrap();

            let found = DeletedItem::find_first(
                "WHERE remote_id = ? AND item_type = ?",
                params!["msg_123", DeletedItemType::Message],
                tx,
            )
            .await
            .unwrap();

            assert!(found.is_some());
            let found = found.unwrap();
            assert_eq!(found.remote_id, "msg_123");
            assert_eq!(found.item_type, DeletedItemType::Message);

            Ok(())
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_deleted_item_save_duplicate() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Save the same item twice
            let mut item1 = DeletedItem::new("msg_123".to_string(), DeletedItemType::Message);
            item1.save(tx).await.unwrap();

            let mut item2 = DeletedItem::new("msg_123".to_string(), DeletedItemType::Message);
            item2.save(tx).await.unwrap();

            // Should only have one item in the database
            let all_items = DeletedItem::all(tx).await.unwrap();
            assert_eq!(all_items.len(), 1);

            Ok(())
        })
        .await
        .unwrap();
}

#[test_case(vec!["msg_1", "msg_2", "msg_3"], vec!["msg_1", "msg_3"], vec!["msg_1", "msg_3"]; "Some messages deleted")]
#[test_case(vec!["msg_1", "msg_2", "msg_3"], vec!["msg_4", "msg_5"], vec![]; "No messages deleted")]
#[test_case(vec!["msg_1", "msg_2", "msg_3"], vec!["msg_1", "msg_2", "msg_3"], vec!["msg_1", "msg_2", "msg_3"]; "All messages deleted")]
#[test_case(vec![], vec!["msg_1", "msg_2"], vec![]; "Empty query list")]
#[tokio::test]
async fn test_find_deleted_by_remote_ids(
    query_ids: Vec<&str>,
    deleted_ids: Vec<&str>,
    expected_found: Vec<&str>,
) {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            for id in deleted_ids {
                let mut item = DeletedItem::new(id.to_string(), DeletedItemType::Message);
                item.save(tx).await.unwrap();
            }

            Ok(())
        })
        .await
        .unwrap();

    let query_strings: Vec<String> = query_ids.iter().map(|s| s.to_string()).collect();
    let found =
        DeletedItem::find_deleted_by_remote_ids(query_strings, DeletedItemType::Message, &tether)
            .await
            .unwrap();

    let expected_set: std::collections::HashSet<String> =
        expected_found.iter().map(|s| s.to_string()).collect();
    assert_eq!(found, expected_set);
}

#[tokio::test]
async fn test_find_deleted_by_remote_ids_different_types() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Add deleted items of different types with the same ID
            let mut msg = DeletedItem::new("123".to_string(), DeletedItemType::Message);
            msg.save(tx).await.unwrap();

            let mut conv = DeletedItem::new("123".to_string(), DeletedItemType::Conversation);
            conv.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Query for messages only
    let found_messages = DeletedItem::find_deleted_by_remote_ids(
        vec!["123".to_string()],
        DeletedItemType::Message,
        &tether,
    )
    .await
    .unwrap();
    assert_eq!(found_messages.len(), 1);

    // Query for conversations only
    let found_convs = DeletedItem::find_deleted_by_remote_ids(
        vec!["123".to_string()],
        DeletedItemType::Conversation,
        &tether,
    )
    .await
    .unwrap();
    assert_eq!(found_convs.len(), 1);

    // Query for labels (should find none)
    let found_labels = DeletedItem::find_deleted_by_remote_ids(
        vec!["123".to_string()],
        DeletedItemType::Label,
        &tether,
    )
    .await
    .unwrap();
    assert_eq!(found_labels.len(), 0);
}

#[tokio::test]
async fn test_verify_and_cleanup_removes_stale_items() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    let now = UnixTimestamp::now();
    let old_timestamp = now.saturating_sub(90000); // Older than 1 day

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Create old item (should be removed)
            let mut old_item = DeletedItem {
                remote_id: "old_msg".to_string(),
                item_type: DeletedItemType::Message,
                deleted_at: old_timestamp,
            };
            old_item.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Run cleanup
    tether
        .tx::<_, _, StashError>(async |tx| {
            DeletedItem::verify_and_cleanup(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Check that old item was removed
    let remaining = DeletedItem::all(&tether).await.unwrap();
    assert_eq!(remaining.len(), 0);
}

#[tokio::test]
async fn test_verify_and_cleanup_removes_re_added_messages() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();
    let address = create_address(&mut tether).await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Create a deleted item tombstone for a message
            let mut deleted_item =
                DeletedItem::new("msg_123".to_string(), DeletedItemType::Message);
            deleted_item.save(tx).await.unwrap();

            // Create a conversation first (messages require a conversation)
            let mut conv = conversation!(remote_id: Some("conv_for_msg_123".into()));
            conv.save(tx).await.unwrap();

            // Simulate data inconsistency: message exists despite being in deleted_items
            let mut msg = message!(
                remote_id: Some("msg_123".into()),
                local_address_id: address.id(),
                remote_address_id: address.remote_id.clone().unwrap(),
                local_conversation_id: Some(conv.id())
            );
            msg.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Verify message exists before cleanup
    let msg_before = Message::find_by_remote_id("msg_123".to_string().into(), &tether)
        .await
        .unwrap();
    assert!(msg_before.is_some(), "Message should exist before cleanup");

    // Run cleanup - should delete the message from messages table (tombstone is authoritative)
    tether
        .tx::<_, _, StashError>(async |tx| {
            DeletedItem::verify_and_cleanup(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Verify message was deleted from messages table
    let msg_after = Message::find_by_remote_id("msg_123".to_string().into(), &tether)
        .await
        .unwrap();
    assert!(
        msg_after.is_none(),
        "Message should be deleted from messages table to enforce consistency"
    );

    // Tombstone should remain until stale (not cleaned up immediately)
    let remaining = DeletedItem::all(&tether).await.unwrap();
    assert_eq!(
        remaining.len(),
        1,
        "Tombstone should remain until it becomes stale"
    );
    assert_eq!(remaining[0].remote_id, "msg_123");
}

#[tokio::test]
async fn test_verify_and_cleanup_removes_re_added_conversations() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Create a deleted item tombstone for a conversation
            let mut deleted_item =
                DeletedItem::new("conv_123".to_string(), DeletedItemType::Conversation);
            deleted_item.save(tx).await.unwrap();

            // Simulate data inconsistency: conversation exists despite being in deleted_items
            let mut conv = conversation!(remote_id: Some("conv_123".into()));
            conv.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Verify conversation exists before cleanup
    let conv_before = Conversation::find_by_remote_id("conv_123".to_string().into(), &tether)
        .await
        .unwrap();
    assert!(
        conv_before.is_some(),
        "Conversation should exist before cleanup"
    );

    // Run cleanup - should delete the conversation from conversations table (tombstone is authoritative)
    tether
        .tx::<_, _, StashError>(async |tx| {
            DeletedItem::verify_and_cleanup(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Verify conversation was deleted from conversations table
    let conv_after = Conversation::find_by_remote_id("conv_123".to_string().into(), &tether)
        .await
        .unwrap();
    assert!(
        conv_after.is_none(),
        "Conversation should be deleted from conversations table to enforce consistency"
    );

    // Tombstone should remain until stale (not cleaned up immediately)
    let remaining = DeletedItem::all(&tether).await.unwrap();
    assert_eq!(
        remaining.len(),
        1,
        "Tombstone should remain until it becomes stale"
    );
    assert_eq!(remaining[0].remote_id, "conv_123");
}

#[tokio::test]
async fn test_verify_and_cleanup_keeps_valid_tombstones() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Create a recent deleted item that is NOT in the messages table
            let mut deleted_item =
                DeletedItem::new("msg_456".to_string(), DeletedItemType::Message);
            deleted_item.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Run cleanup - should keep the item since it's recent and not re-added
    tether
        .tx::<_, _, StashError>(async |tx| {
            DeletedItem::verify_and_cleanup(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Check that item is still there
    let remaining = DeletedItem::all(&tether).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].remote_id, "msg_456");
}

#[tokio::test]
async fn test_verify_and_cleanup_mixed_scenario() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    let now = UnixTimestamp::now();
    let old_timestamp = now.saturating_sub(90000);
    let address = create_address(&mut tether).await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            // 1. Old deleted item (should be removed - stale)
            let mut old_item = DeletedItem {
                remote_id: "old_msg".to_string(),
                item_type: DeletedItemType::Message,
                deleted_at: old_timestamp,
            };
            <DeletedItem as Model>::insert(&mut old_item, tx)
                .await
                .unwrap();

            // 2. Recent deleted item with leaked message (tombstone is authoritative - message should be deleted)
            let mut re_added_item =
                DeletedItem::new("re_added_msg".to_string(), DeletedItemType::Message);
            re_added_item.save(tx).await.unwrap();

            // Create a conversation first (messages require a conversation)
            let mut conv = conversation!(remote_id: Some("conv_for_re_added".into()));
            conv.save(tx).await.unwrap();

            // Simulate leaked message (data inconsistency)
            let mut msg = message!(
                remote_id: Some("re_added_msg".into()),
                local_address_id: address.id(),
                remote_address_id: address.remote_id.clone().unwrap(),
                local_conversation_id: Some(conv.id())
            );
            msg.save(tx).await.unwrap();

            // 3. Recent valid tombstone (should be kept - valid)
            let mut valid_item =
                DeletedItem::new("valid_tombstone".to_string(), DeletedItemType::Message);
            valid_item.save(tx).await.unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Verify leaked message exists before cleanup
    let leaked_msg_before = Message::find_by_remote_id("re_added_msg".to_string().into(), &tether)
        .await
        .unwrap();
    assert!(
        leaked_msg_before.is_some(),
        "Leaked message should exist before cleanup"
    );

    // Run cleanup
    tether
        .tx::<_, _, StashError>(async |tx| {
            DeletedItem::verify_and_cleanup(tx).await.unwrap();
            Ok(())
        })
        .await
        .unwrap();

    // Verify leaked message was deleted from messages table
    let leaked_msg_after = Message::find_by_remote_id("re_added_msg".to_string().into(), &tether)
        .await
        .unwrap();
    assert!(
        leaked_msg_after.is_none(),
        "Leaked message should be deleted from messages table"
    );

    // Check remaining tombstones
    let remaining = DeletedItem::all(&tether).await.unwrap();
    assert_eq!(
        remaining.len(),
        2,
        "Both re_added_msg and valid_tombstone tombstones should remain"
    );

    let tombstone_ids: Vec<&str> = remaining
        .iter()
        .map(|item| item.remote_id.as_str())
        .collect();
    assert!(
        tombstone_ids.contains(&"re_added_msg"),
        "re_added_msg tombstone should remain until stale"
    );
    assert!(
        tombstone_ids.contains(&"valid_tombstone"),
        "valid_tombstone should remain"
    );
}

#[tokio::test]
async fn test_conversation_delete_tracks_all_messages() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    let address = create_address(&mut tether).await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Create a conversation with 3 messages
            let mut conv = conversation!(remote_id: Some("conv_to_delete".into()));
            conv.save(tx).await.unwrap();

            for i in 1..=3 {
                let mut msg = message!(
                    remote_id: Some(format!("msg_{i}").into()),
                    local_address_id: address.id(),
                    remote_address_id: address.remote_id.clone().unwrap(),
                    local_conversation_id: Some(conv.id())
                );
                msg.save(tx).await.unwrap();
            }

            // Simulate EventPoll conversation deletion
            Conversation::handle_event(
                tx,
                &"conv_to_delete".to_string().into(),
                Action::Delete,
                None,
                &mut Default::default(),
                &Default::default(),
            )
            .await
            .unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Verify conversation is tracked
    let conv_deleted = DeletedItem::find_deleted_by_remote_ids(
        vec!["conv_to_delete".to_string()],
        DeletedItemType::Conversation,
        &tether,
    )
    .await
    .unwrap();
    assert_eq!(conv_deleted.len(), 1);

    // Verify all 3 messages are tracked
    let msgs_deleted = DeletedItem::find_deleted_by_remote_ids(
        vec![
            "msg_1".to_string(),
            "msg_2".to_string(),
            "msg_3".to_string(),
        ],
        DeletedItemType::Message,
        &tether,
    )
    .await
    .unwrap();
    assert_eq!(msgs_deleted.len(), 3);

    // Verify conversation was actually deleted (cascade)
    let conv_exists = Conversation::find_by_remote_id("conv_to_delete".to_string().into(), &tether)
        .await
        .unwrap();
    assert!(conv_exists.is_none());

    // Verify messages were cascade-deleted
    let msg_exists = Message::find_by_remote_id("msg_1".to_string().into(), &tether)
        .await
        .unwrap();
    assert!(msg_exists.is_none());
}

#[tokio::test]
async fn test_conversation_delete_skips_null_remote_id_messages() {
    let (stash, _tempdir) = new_test_connection_file().await;
    let mut tether = stash.connection().await.unwrap();

    let address = create_address(&mut tether).await;

    tether
        .tx::<_, _, StashError>(async |tx| {
            // Create a conversation with both synced and local-only messages
            let mut conv = conversation!(remote_id: Some("conv_mixed_messages".into()));
            conv.save(tx).await.unwrap();

            // Create 2 messages with remote_id (synced to server)
            for i in 1..=2 {
                let mut msg = message!(
                    remote_id: Some(format!("msg_synced_{i}").into()),
                    local_address_id: address.id(),
                    remote_address_id: address.remote_id.clone().unwrap(),
                    local_conversation_id: Some(conv.id())
                );
                msg.save(tx).await.unwrap();
            }

            // Create 2 messages with null remote_id (local-only, not synced)
            for _ in 1..=2 {
                let mut msg = message!(
                    remote_id: None,
                    local_address_id: address.id(),
                    remote_address_id: address.remote_id.clone().unwrap(),
                    local_conversation_id: Some(conv.id())
                );
                msg.save(tx).await.unwrap();
            }

            // Simulate EventPoll conversation deletion
            Conversation::handle_event(
                tx,
                &"conv_mixed_messages".to_string().into(),
                Action::Delete,
                None,
                &mut Default::default(),
                &Default::default(),
            )
            .await
            .unwrap();

            Ok(())
        })
        .await
        .unwrap();

    // Verify conversation is tracked
    let conv_deleted = DeletedItem::find_deleted_by_remote_ids(
        vec!["conv_mixed_messages".to_string()],
        DeletedItemType::Conversation,
        &tether,
    )
    .await
    .unwrap();
    assert_eq!(conv_deleted.len(), 1);

    // Verify only the 2 synced messages are tracked (remote_id IS NOT NULL filter)
    let msgs_deleted = DeletedItem::find_deleted_by_remote_ids(
        vec!["msg_synced_1".to_string(), "msg_synced_2".to_string()],
        DeletedItemType::Message,
        &tether,
    )
    .await
    .unwrap();
    assert_eq!(
        msgs_deleted.len(),
        2,
        "Only messages with remote_id should be tracked"
    );

    // Verify total deleted items count (1 conversation + 2 messages = 3)
    let all_deleted_items = DeletedItem::all(&tether).await.unwrap();
    assert_eq!(
        all_deleted_items.len(),
        3,
        "Should only track conversation and 2 synced messages, not local-only messages"
    );

    // Verify breakdown by type
    let message_tombstones: Vec<_> = all_deleted_items
        .iter()
        .filter(|item| item.item_type == DeletedItemType::Message)
        .collect();
    assert_eq!(
        message_tombstones.len(),
        2,
        "Should have exactly 2 message tombstones"
    );

    let conversation_tombstones: Vec<_> = all_deleted_items
        .iter()
        .filter(|item| item.item_type == DeletedItemType::Conversation)
        .collect();
    assert_eq!(
        conversation_tombstones.len(),
        1,
        "Should have exactly 1 conversation tombstone"
    );
}
