use crate::draft::observers::{DraftAttachmentObserver, DraftSendResultWatcher};
use crate::models::{
    Attachment, Conversation, DraftAttachmentMetadata, DraftMetadata, DraftSendFailure,
    DraftSendFailureSave, DraftSendResult, DraftSendResultOrigin, Message,
};
use proton_api_core::services::proton::AddressId;
use proton_api_mail::services::proton::common::{ConversationId, MessageId};
use proton_core_common::datatypes::{AddressStatus, AddressType};
use proton_core_common::models::Address;
use proton_mail_ids::LocalMessageId;
use proton_mail_test_utils::db::new_test_connection_file;
use stash::stash::{Bond, StashError};
#[tokio::test]
async fn draft_send_observer_only_triggers_for_new_items_empty_db() {
    let (stash, _db_dir) = new_test_connection_file().await;

    let mut conn = stash.connection();
    conn.tx::<_, _, StashError>(async |tx| {
        create_test_messages(2, tx).await;
        Ok(())
    })
    .await
    .unwrap();

    let mut watcher = DraftSendResultWatcher::new(stash.clone()).await.unwrap();

    let mut v1 = DraftSendResult::failure(
        LocalMessageId::from(1),
        DraftSendResultOrigin::Save,
        DraftSendFailure::Internal,
    );
    let mut v2 = DraftSendResult::failure(
        LocalMessageId::from(2),
        DraftSendResultOrigin::Save,
        DraftSendFailure::Internal,
    );

    // insert first record
    conn.tx::<_, _, StashError>(async |tx| {
        v1.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v1.clone()]);

    // insert second record
    conn.tx::<_, _, StashError>(async |tx| {
        v2.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v2.clone()]);

    // Mark first record as seen.
    conn.tx::<_, _, StashError>(async |tx| {
        v1.seen = true;
        v1.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    // There should be no changes.
    tokio::time::timeout(std::time::Duration::from_secs(2), watcher.next())
        .await
        .unwrap_err();
}

#[tokio::test]
async fn draft_send_observer_only_triggers_for_new_items_with_existing() {
    let (stash, _db_dir) = new_test_connection_file().await;

    let mut conn = stash.connection();
    conn.tx::<_, _, StashError>(async |tx| {
        create_test_messages(5, tx).await;

        for i in 1..3_u64 {
            let mut existing = DraftSendResult::failure(
                LocalMessageId::from(i),
                DraftSendResultOrigin::Save,
                DraftSendFailure::Internal,
            );

            existing.save(tx).await.unwrap();
        }
        Ok(())
    })
    .await
    .unwrap();

    let mut watcher = DraftSendResultWatcher::new(stash.clone()).await.unwrap();

    let mut v1 = DraftSendResult::failure(
        LocalMessageId::from(4),
        DraftSendResultOrigin::Save,
        DraftSendFailure::Internal,
    );
    let mut v2 = DraftSendResult::failure(
        LocalMessageId::from(5),
        DraftSendResultOrigin::Save,
        DraftSendFailure::Internal,
    );

    // insert first record
    conn.tx::<_, _, StashError>(async |tx| {
        v1.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v1.clone()]);

    // insert second record
    conn.tx::<_, _, StashError>(async |tx| {
        v2.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v2.clone()]);

    // Mark first record as seen.
    conn.tx::<_, _, StashError>(async |tx| {
        DraftSendResult::mark_seen(std::iter::once(v1.local_message_id), tx)
            .await
            .unwrap();
        Ok(())
    })
    .await
    .unwrap();

    // There should be no changes.
    tokio::time::timeout(std::time::Duration::from_secs(2), watcher.next())
        .await
        .unwrap_err();

    // Delete second record
    conn.tx::<_, _, StashError>(async |tx| {
        DraftSendResult::mark_seen(std::iter::once(v2.local_message_id), tx)
            .await
            .unwrap();
        Ok(())
    })
    .await
    .unwrap();

    // There should be no changes.
    tokio::time::timeout(std::time::Duration::from_secs(2), watcher.next())
        .await
        .unwrap_err();
}

#[tokio::test]
async fn draft_send_observer_re_triggers_for_same_message_with_different_error() {
    let (stash, _db_dir) = new_test_connection_file().await;

    let mut conn = stash.connection();
    conn.tx::<_, _, StashError>(async |tx| {
        create_test_messages(2, tx).await;
        Ok(())
    })
    .await
    .unwrap();

    let mut watcher = DraftSendResultWatcher::new(stash.clone()).await.unwrap();

    let mut v1 = DraftSendResult::failure(
        LocalMessageId::from(1),
        DraftSendResultOrigin::Save,
        DraftSendFailure::Internal,
    );
    let mut v2 = DraftSendResult::failure(
        LocalMessageId::from(1),
        DraftSendResultOrigin::SaveBeforeSend,
        DraftSendFailure::Save(DraftSendFailureSave::MessageDoesNotExist),
    );

    // insert first record
    conn.tx::<_, _, StashError>(async |tx| {
        v1.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v1.clone()]);

    // insert second record
    conn.tx::<_, _, StashError>(async |tx| {
        v2.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v2.clone()]);
}

#[tokio::test]
async fn draft_attachment_observer_updates_when_attachment_is_removed() {
    let (stash, _db_dir) = new_test_connection_file().await;

    let mut conn = stash.connection();
    let mut attachment = Attachment {
        local_id: None,
        attachment_type: Default::default(),
        local_address_id: None,
        remote_address_id: None,
        local_conversation_id: None,
        remote_conversation_id: None,
        local_message_id: None,
        remote_message_id: None,
        disposition: Default::default(),
        enc_signature: None,
        is_auto_forwardee: false,
        key_packets: None,
        mime_type: Default::default(),
        filename: "".to_string(),
        sender: None,
        signature: None,
        size: 0,
        content_id: None,
        transfer_encoding: None,
        image_width: None,
        image_height: None,
        row_id: None,
    };
    let metadata = conn
        .tx::<_, _, StashError>(async |tx| {
            attachment.save(tx).await.unwrap();
            DraftMetadata::empty(tx).await
        })
        .await
        .unwrap();

    let mut watcher = DraftAttachmentObserver::new(metadata.id.unwrap(), stash.clone())
        .await
        .unwrap();

    let mut attachment_metadata =
        DraftAttachmentMetadata::new(metadata.id.unwrap(), attachment.local_id.unwrap(), 0);
    // Create metadata.
    conn.tx::<_, _, StashError>(async |tx| {
        attachment_metadata.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    // Trigger for new attachment.
    tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();

    // Simulate delete
    conn.tx::<_, _, StashError>(async |tx| {
        attachment_metadata.deleted = true;
        attachment_metadata.save(tx).await.unwrap();
        Ok(())
    })
    .await
    .unwrap();

    // Trigger for update.
    tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
}

async fn create_test_messages(count: usize, bond: &Bond<'_>) {
    let mut address = Address {
        local_id: None,
        remote_id: Some(AddressId::from("addr-id".to_owned())),
        address_type: AddressType::Original,
        catch_all: false,
        display_name: "".to_string(),
        display_order: 0,
        domain_id: None,
        email: "".to_string(),
        keys: Default::default(),
        proton_mx: false,
        receive: false,
        send: false,
        signature: "".to_string(),
        signed_key_list: Default::default(),
        status: AddressStatus::Disabled,
        row_id: None,
    };
    address.save(bond).await.unwrap();
    let mut conversation = Conversation {
        remote_id: Some(ConversationId::from("conv-id".to_owned())),
        ..Default::default()
    };
    conversation.save(bond).await.unwrap();
    for i in 0..count {
        let mut message = Message {
            remote_id: Some(MessageId::from(format!("msg-{i}"))),
            local_conversation_id: conversation.local_id,
            remote_conversation_id: conversation.remote_id.clone(),
            local_address_id: address.local_id.unwrap(),
            ..Default::default()
        };
        message.save(bond).await.unwrap();
    }
}
