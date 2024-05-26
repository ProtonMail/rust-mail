use crate::common::{new_mock_remote, DeleteMessageAction, MockRemoteSource, MoveMessageAction, TestCtx, TestLocalSourceTransaction};
use mockall::*;
use proton_api_core::exports::anyhow::anyhow;
use proton_api_core::http::RequestError;
use std::sync::Arc;

mod common;

#[tokio::test]
async fn successive_message_move_but_fails_on_first_remote_action() {
    let ctx = TestCtx::new().await;
    let transaction = ctx.stash().transaction().await.expect("failed to start transaction");
    let mut tx = TestLocalSourceTransaction::new(transaction.clone());
    
        let inbox_id = tx.create_folder("Inbox").await.expect("failed to create folder");
        let folder1_id = tx
            .create_folder("Folder1").await
            .expect("failed to create folder");
        let folder2_id = tx
            .create_folder("Folder2").await
            .expect("failed to create folder");
        let folder3_id = tx
            .create_folder("Folder3").await
            .expect("failed to create folder");

        let message_id = tx.create_message(false).await.expect("failed to create message");

        tx.move_message_to_folder(&[message_id], inbox_id).await
            .expect("failed to move");
    
    transaction.commit().await.expect("failed to commit");

    // remote expectations
    let mut remote = MockRemoteSource::new();
    remote
        .expect_move_messages()
        .with(predicate::eq(folder3_id), predicate::always())
        .returning(|_, _| Err(RequestError::Other(anyhow!("failed to move"))))
        .times(1);

    let queue = ctx.new_action_queue(Arc::new(remote));

    queue
        .queue_action(&MoveMessageAction::new(inbox_id, folder1_id, [message_id])).await
        .expect("failed to add action");
    queue
        .queue_action(&MoveMessageAction::new(
            folder1_id,
            folder2_id,
            [message_id],
        )).await
        .expect("failed to add action");
    queue
        .queue_action(&MoveMessageAction::new(
            folder2_id,
            folder3_id,
            [message_id],
        )).await
        .expect("failed to add action");

    queue.consume_pending().await.expect("failed to consume actions");

        // Message should be back into folder 1.
        let message = tx
            .get_message(message_id).await
            .expect("Failed to get message")
            .expect("Must exist");
        assert_eq!(message.folder, Some(inbox_id));
}

#[tokio::test]
async fn move_message_to_folder_remote_exec_fails() {
    let ctx = TestCtx::new().await;
    let transaction = ctx.stash().transaction().await.expect("failed to start transaction");
    let mut tx = TestLocalSourceTransaction::new(transaction.clone());

        let inbox_id = tx.create_folder("Inbox").await.expect("failed to create folder");
        let folder1_id = tx
            .create_folder("Folder1").await
            .expect("failed to create folder");

        let message_id = tx.create_message(false).await.expect("failed to create message");

        tx.move_message_to_folder(&[message_id], inbox_id).await
            .expect("failed to move");
    
    transaction.commit().await.expect("failed to commit");

    // remote expectations
    let remote = new_mock_remote(|m| {
        m.expect_move_messages()
            .with(predicate::eq(folder1_id), predicate::always())
            .returning(|_, _| Err(RequestError::Other(anyhow!("failed to move"))))
            .times(1);
    });

    let queue = ctx.new_action_queue(remote);

    queue
        .queue_action(&MoveMessageAction::new(inbox_id, folder1_id, [message_id])).await
        .expect("failed to add action");

    queue.consume_pending().await.expect("failed to consume actions");

    // Message should be back into folder 1.
        let message = tx
            .get_message(message_id).await
            .expect("Failed to get message")
            .expect("Must exist");
        assert_eq!(message.folder, Some(inbox_id));
}

#[tokio::test]
async fn successive_message_move_and_succeeds() {
    let ctx = TestCtx::new().await;
    let transaction = ctx.stash().transaction().await.expect("failed to start transaction");
    let mut tx = TestLocalSourceTransaction::new(transaction.clone());

        let inbox_id = tx.create_folder("Inbox").await.expect("failed to create folder");
        let folder1_id = tx
            .create_folder("Folder1").await
            .expect("failed to create folder");
        let folder2_id = tx
            .create_folder("Folder2").await
            .expect("failed to create folder");
        let folder3_id = tx
            .create_folder("Folder3").await
            .expect("failed to create folder");

        let message_id = tx.create_message(false).await.expect("failed to create message");

        tx.move_message_to_folder(&[message_id], inbox_id).await
            .expect("failed to move");
    
    transaction.commit().await.expect("failed to commit");

    // remote expectations
    let remote = new_mock_remote(|m| {
        m.expect_move_messages()
            .with(predicate::eq(folder3_id), predicate::always())
            .returning(|_, _| Ok(()))
            .times(1);
    });

    let queue = ctx.new_action_queue(remote);

    queue
        .queue_action(&MoveMessageAction::new(inbox_id, folder1_id, [message_id])).await
        .expect("failed to add action");
    queue
        .queue_action(&MoveMessageAction::new(
            folder1_id,
            folder2_id,
            [message_id],
        )).await
        .expect("failed to add action");
    queue
        .queue_action(&MoveMessageAction::new(
            folder2_id,
            folder3_id,
            [message_id],
        )).await
        .expect("failed to add action");

    queue.consume_pending().await.expect("failed to consume actions");

        let message = tx
            .get_message(message_id).await
            .expect("Failed to get message")
            .expect("Must exist");
        assert_eq!(message.folder, Some(folder3_id));
}

#[tokio::test]
async fn move_message_to_folder_but_remote_action_occurred_before_execution() {
    let ctx = TestCtx::new().await;
    let transaction = ctx.stash().transaction().await.expect("failed to start transaction");
    let mut tx = TestLocalSourceTransaction::new(transaction.clone());

        let inbox_id = tx.create_folder("Inbox").await.expect("failed to create folder");
        let folder1_id = tx
            .create_folder("Folder1").await
            .expect("failed to create folder");
        let folder2_id = tx
            .create_folder("Folder2").await
            .expect("failed to create folder");

        let message_id = tx.create_message(false).await.expect("failed to create message");

        tx.move_message_to_folder(&[message_id], inbox_id).await
            .expect("failed to move");
    
    transaction.commit().await.expect("failed to commit");

    // remote expectations - nothing
    let remote = new_mock_remote(|_| {});

    let queue = ctx.new_action_queue(remote);

    queue
        .queue_action(&MoveMessageAction::new(inbox_id, folder1_id, [message_id])).await
        .expect("failed to add action");

        // simulate remote action taking place.
        tx.move_message_to_folder(&[message_id], folder2_id).await
            .expect("Failed to move");

    queue.consume_pending().await.expect("failed to consume actions");

        // Message should be back into folder 1.
        let message = tx
            .get_message(message_id).await
            .expect("Failed to get message")
            .expect("Must exist");
        assert_eq!(message.folder, Some(folder2_id));
}

#[tokio::test]
async fn move_message_to_folder_two_actions_interleaved_with_remote_change() {
    let ctx = TestCtx::new().await;
    let transaction = ctx.stash().transaction().await.expect("failed to start transaction");
    let mut tx = TestLocalSourceTransaction::new(transaction.clone());

        let inbox_id = tx.create_folder("Inbox").await.expect("failed to create folder");
        let folder1_id = tx
            .create_folder("Folder1").await
            .expect("failed to create folder");
        let folder2_id = tx
            .create_folder("Folder2").await
            .expect("failed to create folder");
        let folder3_id = tx
            .create_folder("Folder3").await
            .expect("failed to create folder");

        let message_id = tx.create_message(false).await.expect("failed to create message");

        tx.move_message_to_folder(&[message_id], inbox_id).await
            .expect("failed to move");
    
    transaction.commit().await.expect("failed to commit");

    // remote expectations - nothing, remote action superseeds everything.
    let remote = new_mock_remote(|_| {});

    let queue = ctx.new_action_queue(remote);

    queue
        .queue_action(&MoveMessageAction::new(inbox_id, folder1_id, [message_id])).await
        .expect("failed to add action");
    queue
        .queue_action(&MoveMessageAction::new(
            folder1_id,
            folder2_id,
            [message_id],
        )).await
        .expect("failed to add action");

    // consume first action
    queue
        .consume_pending_with_limit(1).await
        .expect("failed to consume actions");

    // Simulate a remote change applied locally
        tx.move_message_to_folder(&[message_id], folder3_id).await
            .expect("Failed to move");

    // Consume next action.
    queue
        .consume_pending_with_limit(1).await
        .expect("failed to consume actions");

    // Message should be back into folder 1.
        let message = tx
            .get_message(message_id).await
            .expect("Failed to get message")
            .expect("Must exist");
        assert_eq!(message.folder, Some(folder3_id));
}

#[tokio::test]
async fn delete_message_queued_action_executed_after_local_change() {
    let ctx = TestCtx::new().await;
    let transaction = ctx.stash().transaction().await.expect("failed to start transaction");
    let mut tx = TestLocalSourceTransaction::new(transaction.clone());

        let inbox_id = tx.create_folder("Inbox").await.expect("failed to create folder");
        let folder1_id = tx
            .create_folder("Folder1").await
            .expect("failed to create folder");

        let message_id = tx.create_message(false).await.expect("failed to create message");

        tx.move_message_to_folder(&[message_id], inbox_id).await
            .expect("failed to move");
    
    transaction.commit().await.expect("failed to commit");

    // remote expectations - nothing, remote action superseeds everything.
    let remote = new_mock_remote(|m| {
        m.expect_delete_messages()
            .times(1)
            .with(predicate::eq([message_id]))
            .returning(|_| Err(RequestError::Other(anyhow!("failed to delete"))));
    });

    let queue = ctx.new_action_queue(remote);

    queue
        .queue_action(&DeleteMessageAction::new([message_id])).await
        .expect("failed to add action");

    // Simulate a remote change applied locally
        tx.move_message_to_folder(&[message_id], folder1_id).await
            .expect("Failed to move");

        let current = tx
            .get_messages(&[message_id]).await
            .expect("failed to get messages");
        assert!(current.is_empty());

    // Consume next action.
    queue.consume_pending().await.expect("failed to consume actions");

        let current = tx
            .get_messages(&[message_id]).await
            .expect("failed to get messages");
        assert!(!current.is_empty());

        // Message should be back into folder 1.
        let message = tx
            .get_message(message_id).await
            .expect("Failed to get message")
            .expect("Must exist");
        println!("{:?}", message);
        assert_eq!(message.folder, Some(folder1_id));
}
