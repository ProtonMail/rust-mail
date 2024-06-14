use crate::store::PendingAction;
use crate::{
    Action, ActionError, ActionFactory, ActionFactoryError, ActionLocalValidationResult,
    ActionStore, SessionProvider, StoredActionId,
};
use proton_api_core::exports::thiserror;
use proton_sqlite3::rusqlite;
use stash::stash::{Stash, StashError};
use tracing::{debug, error, warn, Level};

/// Errors which can occur while operating on the queue.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("Action failed: {0}")]
    Action(#[from] ActionError),
    #[error("Queue Store failed: {0}")]
    Store(#[from] rusqlite::Error),
    #[error("Factory: {0}")]
    Factory(#[from] ActionFactoryError),
    #[error("Stash: {0}")]
    Stash(#[from] StashError),
}

pub type ActionQueueResult<T> = Result<T, QueueError>;

pub struct ActionQueue {
    pub stash: Stash,
    session_provider: Box<dyn SessionProvider>,
    action_factory: ActionFactory,
}

impl ActionQueue {
    pub fn new(
        stash: Stash,
        session_provider: Box<dyn SessionProvider>,
        action_factory: ActionFactory,
    ) -> Self {
        Self {
            stash,
            session_provider,
            action_factory,
        }
    }

    pub async fn queue_action<T: Action>(&self, action: &T) -> ActionQueueResult<StoredActionId> {
        let span = tracing::span!(Level::DEBUG, "Queue Action", action = ?action, action_id=action.action_id().to_string());
        let _entered = span.enter();
        let tx = self.stash.transaction().await.map_err(|e| {
            error!("Failed to start transaction: {e}");
            e
        })?;
        {
            let mut store = ActionStore::new(tx.clone());
            let pending_action =
                PendingAction::from_action(action).map_err(ActionError::Serialization)?;

            // Write action to store
            let id = store.store_action(pending_action).await?;

            {
                let mut handler = self.action_factory.local_handler(action, tx.clone())?;

                // Apply locally
                if let Err(e) = handler.apply_local() {
                    error!("Failed to apply local changes: {e}");
                    return Err(e.into());
                }
            }
            // Done
            debug!("action stored id={id}");
            tx.commit().await?;
            Ok(id)
        }
        .map_err(|e| {
            if let QueueError::Store(e) = &e {
                error!("Failed to commit changes: {e}");
            }
            e
        })
    }
    pub async fn consume_pending(&self) -> ActionQueueResult<()> {
        while self.consume_pending_impl().await? {}
        Ok(())
    }

    pub async fn consume_pending_with_limit(&self, limit: usize) -> ActionQueueResult<()> {
        for _ in 0..limit {
            if !self.consume_pending_impl().await? {
                return Ok(());
            }
        }

        Ok(())
    }

    async fn consume_pending_impl(&self) -> ActionQueueResult<bool> {
        let span = tracing::span!(Level::DEBUG, "consume_pending");
        let _entered = span.enter();
        let tx = self.stash.transaction().await.map_err(|e| {
            error!("Failed to start transaction: {e}");
            e
        })?;
        {
            let mut store = ActionStore::new(tx.clone());
            // Load pending actions from store
            let Some(pending) = store.get_next_action().await? else {
                debug!("No actions to consume");
                return Ok(false);
            };
            let Some(action_id) = pending.id else {
                warn!("Missing action id");
                return Ok(false);
            };

            let action_span =
                tracing::span!(Level::DEBUG, "action", stored_id = action_id.to_string());
            action_span.in_scope(|| -> ActionQueueResult<()> {
                let mut handler = self
                    .action_factory
                    .remote_handler(pending.clone(), tx.clone(), self.session_provider.as_ref())
                    .map_err(|e| {
                        error!("Failed to create handler: {e}");
                        e
                    })?;

                // Check if state is still correct
                if handler.validate_local()? == ActionLocalValidationResult::Invalid {
                    warn!("action state is no longer valid skipping");
                } else {
                    // If yes, apply remote
                    if let Err(e) = handler.apply_remote() {
                        error!("Failed to apply action remotely: {e}");
                        // If remote fails revert
                        if let Err(e) = handler.revert_local() {
                            // Log revert local change. Things are unstable.
                            error!("Failed to revert action locally:{e}");
                            return Err(e.into());
                        }
                        debug!("Action reverted");
                    }
                }
                Ok(())
            })?;

            if let Err(e) = store.erase_actions(&[action_id]).await {
                error!("Failed to remove action: {e}");
                return Err(e.into());
            }

            debug!("Erased pending action");

            tx.commit().await?;
            Ok(true)
        }
        .map_err(|e| {
            if let QueueError::Store(e) = &e {
                error!("Failed to commit changes: {e}");
            }
            e
        })
    }
}
