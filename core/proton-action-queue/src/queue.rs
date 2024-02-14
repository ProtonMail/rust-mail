use crate::store::PendingAction;
use crate::{
    Action, ActionError, ActionFactory, ActionFactoryError, ActionLocalValidationResult,
    ActionPriority, ActionStore, SessionProvider, StoredActionId,
};
use proton_api_core::exports::thiserror;
use proton_sqlite3::{rusqlite, MigratorError};
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
    #[error("DB Migration: {0}")]
    Migration(#[from] MigratorError),
}

pub type ActionQueueResult<T> = Result<T, QueueError>;

pub struct ActionQueue {
    connection: proton_sqlite3::SqliteConnection,
    session_provider: Box<dyn SessionProvider>,
    action_factory: ActionFactory,
}

impl ActionQueue {
    pub fn new(
        mut connection: proton_sqlite3::SqliteConnection,
        session_provider: Box<dyn SessionProvider>,
        action_factory: ActionFactory,
    ) -> ActionQueueResult<Self> {
        ActionStore::init_tables(&mut connection)?;
        Ok(Self {
            connection,
            session_provider,
            action_factory,
        })
    }

    pub fn queue_action<T: Action>(
        &mut self,
        action: &T,
        priority: ActionPriority,
    ) -> ActionQueueResult<StoredActionId> {
        let mut store = ActionStore::new(&mut self.connection)?;
        let span = tracing::span!(Level::DEBUG, "Queue Action", action = ?action, action_id=action.action_id().to_string());

        span.in_scope(|| -> ActionQueueResult<StoredActionId> {
            let pending_action =
                PendingAction::from_action(action, priority).map_err(ActionError::Serialization)?;

            // Write action to store
            let id = store.store_action(pending_action)?;

            {
                let mut handler = self.action_factory.local_handler(action, store.tx())?;

                // Apply locally
                if let Err(e) = handler.apply_local() {
                    error!("Failed to apply local changes: {e}");
                    return Err(e.into());
                }
            }

            if let Err(e) = store.commit() {
                error!("Failed to commit changes: {e}");
                return Err(e.into());
            }

            // Done
            debug!("action stored id={id}");
            Ok(id)
        })
    }
    pub fn consume_pending(&mut self) -> ActionQueueResult<()> {
        while self.consume_pending_impl()? {}
        Ok(())
    }

    pub fn consume_pending_with_limit(&mut self, limit: usize) -> ActionQueueResult<()> {
        for _ in 0..limit {
            if !self.consume_pending_impl()? {
                return Ok(());
            }
        }

        Ok(())
    }

    fn consume_pending_impl(&mut self) -> ActionQueueResult<bool> {
        let mut store = ActionStore::new(&mut self.connection)?;
        let span = tracing::span!(Level::DEBUG, "consume_pending");
        span.in_scope(|| -> ActionQueueResult<bool> {
            // Load pending actions from store
            let Some(pending) = store.get_next_action()? else {
                debug!("No actions to consume");
                return Ok(false);
            };

            let action_span =
                tracing::span!(Level::DEBUG, "action", stored_id = pending.id.to_string());
            action_span.in_scope(|| -> ActionQueueResult<()> {
                let mut handler = self
                    .action_factory
                    .remote_handler(&pending, store.tx(), self.session_provider.as_ref())
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

            if let Err(e) = store.erase_actions(&[pending.id]) {
                error!("Failed to remove action: {e}");
                return Err(e.into());
            }

            debug!("Erased pending action");

            if let Err(e) = store.commit() {
                error!("Failed to commit changes: {e}");
                return Err(e.into());
            }

            Ok(true)
        })
    }

    #[cfg(test)]
    pub fn get_store(&mut self) -> ActionStore<'_> {
        ActionStore::new(&mut self.connection).expect("failed to get action store")
    }
}
