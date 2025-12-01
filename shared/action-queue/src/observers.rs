//! Implementation of common observer patterns that are useful to follow queued actions.
use crate::action::{Action, ActionId};
use crate::queue::{BroadcastMessage, Queue, QueuedMetadata};
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::broadcast::{Receiver, error::RecvError};

/// This observer only reports failures of a given action type.
pub struct ActionFailureObserver<T: Action> {
    receiver: Receiver<BroadcastMessage>,
    p: PhantomData<T>,
}

/// Reason why the action failed.
#[derive(Debug)]
pub enum ActionFailureReason {
    /// Execution error.
    Error(Arc<anyhow::Error>, Arc<QueuedMetadata>),
    /// Cancelled via user request or dependency execution error.
    Cancelled(Arc<QueuedMetadata>),
    /// Deleted via user request.
    Deleted(ActionId),
}

impl<T: Action> ActionFailureObserver<T> {
    /// Create a new instance which observes a given `queue`
    #[must_use]
    pub fn new(queue: &Queue) -> Self {
        Self {
            receiver: queue.new_broadcast_receiver(),
            p: PhantomData,
        }
    }

    /// Await the next failure of action of type `T`.
    pub async fn next(&mut self) -> Result<ActionFailureReason, RecvError> {
        loop {
            match self.receiver.recv().await {
                Ok(msg) => match msg {
                    BroadcastMessage::Error(err, meta) if meta.action_type == T::TYPE.as_ref() => {
                        return Ok(ActionFailureReason::Error(err, meta));
                    }
                    BroadcastMessage::Cancelled(meta) if meta.action_type == T::TYPE.as_ref() => {
                        return Ok(ActionFailureReason::Cancelled(meta));
                    }
                    BroadcastMessage::Deleted(id, action_type)
                        if action_type.as_ref() == T::TYPE.as_ref() =>
                    {
                        return Ok(ActionFailureReason::Deleted(id));
                    }
                    _ => {}
                },
                Err(RecvError::Closed) => {
                    return Err(RecvError::Closed);
                }
                Err(_) => {}
            }
        }
    }
}

/// Wait for a given action to complete.
///
/// Completion does not necessary mean the action finished executing correctly.
pub struct ActionAwaiter {
    receiver: Receiver<BroadcastMessage>,
}

impl ActionAwaiter {
    /// Create a new instance to wait on the action with `action_id` queue in the given `queue`.
    #[must_use]
    pub fn new(queue: &Queue) -> Self {
        Self {
            receiver: queue.new_broadcast_receiver(),
        }
    }

    /// Wait on the action to finish executing.
    ///
    /// Returns the message which signaled the execution completion. Inspect to determine
    /// if was successful or not.
    ///
    /// # Remarks
    ///
    /// It's theoretically possible to create a waiter while the action is executing and missing
    /// the broadcast. It's recommended call this method in a select statement with some other
    /// exit condition.
    pub async fn wait(&mut self, action_id: ActionId) -> Result<BroadcastMessage, RecvError> {
        loop {
            match self.receiver.recv().await {
                Ok(msg) => match &msg {
                    BroadcastMessage::Success(id, _) | BroadcastMessage::Deleted(id, _)
                        if *id == action_id =>
                    {
                        return Ok(msg);
                    }
                    BroadcastMessage::Error(_, meta) | BroadcastMessage::Cancelled(meta)
                        if meta.id == action_id =>
                    {
                        return Ok(msg);
                    }
                    _ => {}
                },
                Err(RecvError::Closed) => {
                    return Err(RecvError::Closed);
                }
                Err(_) => {}
            }
        }
    }
}
