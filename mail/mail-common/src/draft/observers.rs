use crate::actions::draft;
use crate::models::MetadataId;
use proton_action_queue::action::Action;
use proton_action_queue::observers::{ActionFailureObserver, ActionFailureReason};
use proton_action_queue::queue::{ActionError, AsActionError, Queue};
use tokio::sync::broadcast::error::RecvError;

/// Errors that may occur during observation.
#[derive(Debug, thiserror::Error)]
pub enum ObserverError {
    #[error("Connection with queue lost")]
    RecvError(#[from] RecvError),
    #[error("Failed to deserialize metadata: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),
    #[error("Action failed but metadata was not set")]
    MetadataMissing,
}

/// Information associated with the draft failure.
pub struct DraftFailureInfo {
    /// Metadata id of the draft.
    pub draft_metadata_id: MetadataId,
    pub reason: ActionFailureReason,
}

impl DraftFailureInfo {
    /// Extract the action error from failure reason.
    pub fn error<T: Action>(&self) -> Option<&ActionError<T>> {
        if let ActionFailureReason::Error(error, _) = &self.reason {
            error.as_action_error::<T>()
        } else {
            None
        }
    }
}

/// Observe failure for queued save draft actions.
pub struct DraftSaveFailureObserver {
    observer: ActionFailureObserver<draft::Save>,
}

impl DraftSaveFailureObserver {
    /// Create a new instance with the given `queue`.
    pub fn new(queue: &Queue) -> Self {
        Self {
            observer: ActionFailureObserver::new(queue),
        }
    }

    /// Await the next failure of a draft save action.
    ///
    /// # Errors
    ///
    /// Returns error if the connection to the queue has been severed, the metadata in the
    /// action is missing or can't be decoded.
    pub async fn next(&mut self) -> Result<DraftFailureInfo, ObserverError> {
        observer_loop::<draft::Save>(&mut self.observer).await
    }
}

/// Observe failures for queued send draft actions.
pub struct DraftSendFailureObserver {
    observer: ActionFailureObserver<draft::Send>,
}
impl DraftSendFailureObserver {
    /// Create a new instance with the given `queue`.
    pub fn new(queue: &Queue) -> Self {
        Self {
            observer: ActionFailureObserver::new(queue),
        }
    }

    /// Await the next failure of a draft send action.
    ///
    /// # Errors
    ///
    /// Returns error if the connection to the queue has been severed, the metadata in the
    /// action is missing or can't be decoded.
    pub async fn next(&mut self) -> Result<DraftFailureInfo, ObserverError> {
        observer_loop(&mut self.observer).await
    }
}

async fn observer_loop<T: Action>(
    observer: &mut ActionFailureObserver<T>,
) -> Result<DraftFailureInfo, ObserverError> {
    loop {
        let failure = observer.next().await?;
        match &failure {
            ActionFailureReason::Error(_, m) | ActionFailureReason::Cancelled(m) => {
                if m.resources.is_empty() {
                    return Err(ObserverError::MetadataMissing);
                }
                let metadata_id = m.resources.get::<MetadataId>(0)?;
                return Ok(DraftFailureInfo {
                    draft_metadata_id: metadata_id,
                    reason: failure,
                });
            }
            ActionFailureReason::Deleted(_) => {
                // Deleted is ignored as this implies some corrections are being
                // made to the queue.
            }
        }
    }
}
