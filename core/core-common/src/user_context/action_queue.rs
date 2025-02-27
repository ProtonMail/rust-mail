use std::ops::Deref;

use crate::models::LabelError;
use proton_action_queue::action::WriterGuardError;
use proton_action_queue::queue::{Queue, QueueAutoExecutor};
use proton_api_core::service::ApiServiceError;
use stash::stash::{Stash, StashError};

use super::CoreContextError;

#[allow(dead_code)]
pub struct ActionQueueContext {
    pub action_queue: Queue,
    pub queue_executor: QueueAutoExecutor,
}

impl ActionQueueContext {
    pub async fn new(user_stash: Stash) -> Result<Self, CoreContextError> {
        let action_queue = Queue::new(user_stash).await?;
        let queue_executor = action_queue.new_executor().into_auto_executor();

        Ok(Self {
            action_queue,
            queue_executor,
        })
    }
}

impl Deref for ActionQueueContext {
    type Target = Queue;

    fn deref(&self) -> &Self::Target {
        &self.action_queue
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CoreActionError {
    #[error("Http: {0}")]
    Http(#[from] ApiServiceError),
    #[error("Stash: {0}")]
    Stash(#[from] StashError),
    #[error("Label: {0}")]
    Label(#[from] LabelError),
    #[error("No input provided")]
    NoInput,
    #[error("Queue Writer Guard Expired")]
    QueueWriterGuardExpired,
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl proton_action_queue::action::Error for CoreActionError {
    fn is_network_failure(&self) -> bool {
        if let Self::Http(e) = self {
            e.is_network_failure()
        } else {
            false
        }
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, Self::QueueWriterGuardExpired)
    }
}

impl From<WriterGuardError> for CoreActionError {
    fn from(value: WriterGuardError) -> Self {
        match value {
            WriterGuardError::Expired => Self::QueueWriterGuardExpired,
            WriterGuardError::Stash(e) => Self::Stash(e),
        }
    }
}
