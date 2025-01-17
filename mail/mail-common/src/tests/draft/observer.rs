use crate::draft::observers::DraftSendResultWatcher;
use crate::models::{
    Conversation, DraftSendFailure, DraftSendResult, DraftSendResultOrigin, Message,
};
use proton_api_core::services::proton::common::AddressId;
use proton_api_mail::services::proton::common::{ConversationId, MessageId};
use proton_core_common::datatypes::{AddressStatus, AddressType};
use proton_core_common::models::Address;
use proton_mail_ids::LocalMessageId;
use proton_mail_test_utils::db::new_test_connection_file;
use stash::stash::Bond;
#[tokio::test]
async fn draft_send_observer_only_triggers_for_new_items_empty_db() {
    let (stash, _db_dir) = new_test_connection_file().await;

    let mut conn = stash.connection();
    let tx = conn.transaction().await.unwrap();
    create_test_messages(2, &tx).await;
    tx.commit().await.unwrap();

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
    let tx = conn.transaction().await.unwrap();
    v1.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v1.clone()]);

    // insert second record
    let tx = conn.transaction().await.unwrap();
    v2.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v2.clone()]);

    // Mark first record as seen.
    let tx = conn.transaction().await.unwrap();
    v1.seen = true;
    v1.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    // There should be no changes.
    tokio::time::timeout(std::time::Duration::from_secs(2), watcher.next())
        .await
        .unwrap_err();
}

#[tokio::test]
async fn draft_send_observer_only_triggers_for_new_items_with_existing() {
    let (stash, _db_dir) = new_test_connection_file().await;

    let mut conn = stash.connection();
    let tx = conn.transaction().await.unwrap();
    create_test_messages(5, &tx).await;

    for i in 1..3_u64 {
        let mut existing = DraftSendResult::failure(
            LocalMessageId::from(i),
            DraftSendResultOrigin::Save,
            DraftSendFailure::Internal,
        );

        existing.save(&tx).await.unwrap();
    }
    tx.commit().await.unwrap();

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
    let tx = conn.transaction().await.unwrap();
    v1.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v1.clone()]);

    // insert second record
    let tx = conn.transaction().await.unwrap();
    v2.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v2.clone()]);

    // Mark first record as seen.
    let tx = conn.transaction().await.unwrap();
    DraftSendResult::mark_seen(std::iter::once(v1.local_message_id), &tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // There should be no changes.
    tokio::time::timeout(std::time::Duration::from_secs(2), watcher.next())
        .await
        .unwrap_err();

    // Delete second record
    let tx = conn.transaction().await.unwrap();
    DraftSendResult::mark_seen(std::iter::once(v2.local_message_id), &tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // There should be no changes.
    tokio::time::timeout(std::time::Duration::from_secs(2), watcher.next())
        .await
        .unwrap_err();
}

#[tokio::test]
async fn draft_send_observer_re_triggers_for_same_message_with_different_error() {
    let (stash, _db_dir) = new_test_connection_file().await;

    let mut conn = stash.connection();
    let tx = conn.transaction().await.unwrap();
    create_test_messages(2, &tx).await;
    tx.commit().await.unwrap();

    let mut watcher = DraftSendResultWatcher::new(stash.clone()).await.unwrap();

    let mut v1 = DraftSendResult::failure(
        LocalMessageId::from(1),
        DraftSendResultOrigin::Save,
        DraftSendFailure::Internal,
    );
    let mut v2 = DraftSendResult::failure(
        LocalMessageId::from(1),
        DraftSendResultOrigin::SaveBeforeSend,
        DraftSendFailure::MessageDoesNotExist,
    );

    // insert first record
    let tx = conn.transaction().await.unwrap();
    v1.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v1.clone()]);

    // insert second record
    let tx = conn.transaction().await.unwrap();
    v2.save(&tx).await.unwrap();
    tx.commit().await.unwrap();

    let new_values = tokio::time::timeout(std::time::Duration::from_secs(5), watcher.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_values, vec![v2.clone()]);
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
