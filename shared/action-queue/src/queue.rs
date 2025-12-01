#[cfg(test)]
#[path = "tests/queue.rs"]
mod tests;

use crate::action::{
    Action, ActionGroup, ActionId, Error as ActionErrorTrait, Factory, FactoryError, FactoryResult,
    Handler, LocalOutput, Metadata, Priority, Resources, WriterGuard, WriterGuardError,
};
use crate::db::{
    self, ActionDependency, DEFAULT_LOCK_TIMEOUT, DependencyType, ExecutionGuard, StoredAction,
};
use crate::rebase::RebaseChangeSet;
use anyhow::anyhow;
use bitflags::bitflags;
use chrono::DateTime;
use parking_lot::RwLock;
use proton_sqlite3::MigratorError;
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashError, Tether};
use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use topological_sort::TopologicalSort;
use tracing::{Instrument, debug, error, info};
use uuid::Uuid;

/// Errors which can occur while operating on the queue.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Migration error: {0}")]
    Migrator(#[from] MigratorError),
    #[error("DB Error: {0}")]
    DB(#[from] StashError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),
    #[error("Unknown action: {0}")]
    UnknownAction(String),
    #[error("Cyclic Dependency detected")]
    CyclicDependency,
}

/// Errors that result from queuing or apply actions via the queue.
#[derive(thiserror::Error)]
pub enum ActionError<T: Action> {
    /// The execution of the action failed.
    #[error("{0}")]
    Action(T::Error),
    /// An operation on the queue failed.
    #[error("{0}")]
    Queue(#[from] Error),
}

#[derive(thiserror::Error, Debug)]
pub enum MultiActionError {
    /// The execution of the action failed.
    #[error("{0}")]
    Action(#[from] anyhow::Error),
    /// An operation on the queue failed.
    #[error("{0}")]
    Queue(#[from] Error),
}

impl<T: Action> From<ActionError<T>> for MultiActionError {
    fn from(value: ActionError<T>) -> Self {
        match value {
            ActionError::Action(err) => {
                MultiActionError::Action(anyhow::anyhow!("Error executing {}: {err:?}", T::TYPE))
            }
            ActionError::Queue(error) => MultiActionError::Queue(error),
        }
    }
}

impl From<StashError> for MultiActionError {
    fn from(value: StashError) -> Self {
        Self::Action(anyhow!("Stash error: {value:?}"))
    }
}

// Custom debug impl, otherwise T also needs to have Debug when it is not really necessary.
impl<T: Action> fmt::Debug for ActionError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionError::Action(err) => {
                write!(
                    f,
                    "ActionError::Action{{{err:?}}}: Error executing {}",
                    T::TYPE
                )
            }
            ActionError::Queue(err) => {
                write!(
                    f,
                    "ActionError::Queue{{{err:?}}}: Error executing {}",
                    T::TYPE
                )
            }
        }
    }
}

impl<T: Action> From<StashError> for ActionError<T> {
    fn from(value: StashError) -> Self {
        Self::Queue(value.into())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors that may arise from executing queued actions.
#[derive(Debug, thiserror::Error)]
pub enum QueuedError {
    #[error("Factory Error (ActionId={0}): {1}")]
    Factory(ActionId, FactoryError),
    #[error("Queued Action error: {0}")]
    Action(Arc<anyhow::Error>, Arc<QueuedMetadata>),
    #[error("DB Error: {0}")]
    DB(#[from] StashError),
    #[error("Action {0} does not exist")]
    ActionNotFound(ActionId),
    #[error("Action {0} is being executed")]
    ActionInExecution(ActionId),
}

/// Helper trait to extract errors from queued actions.
pub trait AsActionError {
    /// Extract the specified action's error from the error type.
    ///
    /// If the error is not present or can not be converted into, `None` should be returned.
    fn as_action_error<T: Action>(&self) -> Option<&ActionError<T>>;
}

impl AsActionError for anyhow::Error {
    fn as_action_error<T: Action>(&self) -> Option<&ActionError<T>> {
        self.downcast_ref::<ActionError<T>>()
    }
}

pub type QueuedResult<T> = Result<T, QueuedError>;

/// Metadata associated with a queued [`Action`].
#[derive(Debug)]
pub struct QueuedMetadata {
    /// Identifier of the stored action.
    pub id: ActionId,
    /// Unique identifier for this action
    pub action_type: String,
    /// Version of the stored action.
    pub version: u32,
    /// Datetime when the action was created.
    pub created: DateTime<chrono::Utc>,
    /// Datetime when the action was scheduled for execution.
    ///
    /// This value will be different from `created` if a delay was specified.
    pub scheduled: DateTime<chrono::Utc>,
    /// Priority of the stored action.
    pub priority: Priority,
    /// Other actions that this action depends on.
    ///
    /// Note that this only includes actions that have not yet executed.
    pub dependencies: Vec<ActionDependency>,
    /// Optional debug string associated with this action.
    pub debug_string: Option<String>,
    /// Resources which were associated with this action.
    pub resources: Resources,
    /// Execution group for this action.
    pub action_group: String,
}

impl From<StoredAction> for QueuedMetadata {
    fn from(value: StoredAction) -> Self {
        Self {
            id: value.id.unwrap(),
            action_type: value.action_type,
            version: value.version,
            created: value.created,
            scheduled: value.scheduled,
            priority: value.priority,
            dependencies: value.dependencies,
            debug_string: value.debug_string,
            resources: value.resources,
            action_group: value.action_group,
        }
    }
}

/// Broadcast message issued when actions are executed in the background so their
/// progress can be tracked and potentially awaited on.
#[derive(Debug, Clone)]
pub enum BroadcastMessage {
    /// A new action was queued in this process.
    Queued(ActionId, Arc<QueuedMetadata>),
    /// This queued action was executed successfully
    Success(ActionId, Arc<QueuedMetadata>),
    /// This queued action failed to execute.
    ///
    /// Id of the action is available in the metadata.
    Error(Arc<anyhow::Error>, Arc<QueuedMetadata>),
    /// This action was cancelled.
    Cancelled(Arc<QueuedMetadata>),
    /// This action was deleted.
    Deleted(ActionId, Arc<String>),
}

/// Provides a priority based queue for queuing and/or executing [`Action`].
///
/// The queue ensure that each submitted [`Action`] applies their local and remote changes in the
/// following order:
/// * [`Handler::apply_local`]
/// * [`Handler::apply_remote`]
///   * If this fails [`Handler::revert_local`] is called.
/// * [`Handler::apply_local_post_remote`]
///
///
/// When queueing an action the local changes are applied first and the action is queued for
/// execution as soon as all the conditions are met (Priority, delay and/or dependencies). Queued
/// actions are persisted into the database until they are executed, cancelled or deleted.
///
///
/// See:
/// * [`Queue::queue_action()`]
/// * [`Queue::queue_action_with_metadata()`]
/// * [`Queue::cancel()`]
/// * [`Queue::cancel_with_dependees()`]
/// * [`Queue::delete_action()`]
///
/// ## Executing Queued actions
///
/// Execution of the action requires a [`QueueExecutor`] which will pop actions from the
/// queue and execute them. There is no upper limit on the amount of executors that can
/// be created.
///
/// See:
/// * [`QueueExecutor::execute_one()`]
/// * [`QueueExecutor::execute_all()`]
/// * [`Queue::new_executor`]
///
/// ## Error Handling
///
/// When a queued action fails to execute the error and the [`QueuedMetadata`] type will be
/// returned. The latter contains all the information present in [`Metadata`] and metadata from
/// an [`Action`].
///
/// If you know the type of the action you can retrieve the action error from [`QueuedError`]
/// using the [`QueuedError::action_error`] function. Alternatively you  can also
/// extract the error from [`anyhow::Error`] directly using the [`AsActionError`] extension.
///
/// # Remarks
///
/// There can only be on queue per database connection. Multiple queues in the same database
/// are currently not supported. You can achieve the illusion of multiple queues by assigning your
/// actions to a specific [`ActionGroup`] and creating a [`QueueExecutor`] to operate on this
/// group.
///
pub struct Queue {
    shared: Arc<Shared>,
}

/// Internal shared data used by the [`Queue`] and [`BackgroundWorker`].
pub(crate) struct Shared {
    stash: Stash,
    factory: RwLock<Factory>,
    broadcast_sender: tokio::sync::broadcast::Sender<BroadcastMessage>,
    queued_action_notifier: tokio::sync::Notify,
}

impl Shared {
    fn handler<T>(&self) -> Option<Arc<T::Handler>>
    where
        T: Action,
    {
        self.factory.read().handler::<T>()
    }
}

/// Output of the [`Action`] after being applied with [`Queue::apply_action`] or
/// [`Queue::apply_action_with_metadata`].
pub enum ActionRemoteOutput<Remote> {
    /// Action was executed successfully on local and on remote.
    Executed(Remote),
    /// Action could not be executed on the remote at this time and was queued.
    Queued(ActionId, ActionRequeueReason),
}

/// Output of queueing the [`Action`] with [`Queue::queue_action`] or
/// [`Queue::queue_action_with_metadata`].
///
pub struct QueuedActionOutput<T: Action> {
    /// Result of executing the action locally.
    pub local: T::LocalOutput,
    /// Id of the queued action.
    pub id: ActionId,
}

impl Queue {
    pub async fn tether(&self) -> Result<Tether, StashError> {
        self.shared.stash.connection().await
    }

    /// Create a new queue with the given `stash`;
    pub async fn new(stash: Stash) -> Result<Self> {
        Self::with_factory(stash, Factory::default()).await
    }

    /// Create a new queue with the given `stash` and `factory`;
    pub async fn with_factory(stash: Stash, factory: Factory) -> Result<Self> {
        let mut tether = stash.connection().await?;

        db::migrate(&mut tether).await?;

        let (sender, _) = tokio::sync::broadcast::channel(32);

        let shared = Arc::new(Shared {
            stash,
            factory: RwLock::new(factory),
            broadcast_sender: sender,
            queued_action_notifier: tokio::sync::Notify::new(),
        });

        Ok(Self { shared })
    }

    /// Register an [`Action`] with the factory.
    pub fn register<T: Action>(&self, handler: T::Handler) -> FactoryResult<()> {
        self.shared.factory.write().register::<T>(handler)
    }

    /// Register an [`Action`] with the factory and replace the current record
    /// if it already exists.
    pub fn register_or_replace<T: Action>(&self, handler: T::Handler) {
        self.shared
            .factory
            .write()
            .register_or_replace::<T>(handler);
    }

    /// Return the database associated with the queue.
    #[must_use]
    pub fn stash(&self) -> &Stash {
        &self.shared.stash
    }

    /// # Warning
    ///
    /// This operation does not operate within execution guards. It is intended to be used
    /// before queue executor is resumed (during app initialization). Use with caution.
    pub async fn delete_all_in_group(this: &Self, action_group: ActionGroup) -> QueuedResult<()> {
        let mut tether = this.shared.stash.connection().await?;
        tether
            .tx(async |tx| StoredAction::delete_all_in_group(tx, action_group).await)
            .await?;
        Ok(())
    }

    /// # Warning
    ///
    /// This operation does not operate within execution guards. It is intended to be used
    /// before queue executor is resumed (during app initialization). Use with caution.
    pub async fn delete_all_by_type<T: Action>(&self) -> QueuedResult<usize> {
        let mut tether = self.shared.stash.connection().await?;
        Ok(tether
            .tx(async |tx| StoredAction::delete_by_type(tx, &T::TYPE).await)
            .await?)
    }

    /// Queue an `action` for execution at a later time.
    ///
    /// A default [`Metadata`] type is assigned to this `action`.
    pub async fn queue_action<T: Action>(&self, action: T) -> LocalOutput<T> {
        self.queue_action_with_metadata::<T>(action, Metadata::default())
            .await
    }

    /// Queue actions of the same type sequentially, where each action is a dependency of the next.
    ///
    /// If one fails, everything is rolled back.
    ///
    /// Additionally, a `last_id` arg can be provided to say what this should depend on.
    pub async fn queue_actions<T: Action>(
        &self,
        actions: impl IntoIterator<Item = T>,
        mut last_id: Option<ActionId>,
    ) -> Result<Vec<QueuedActionOutput<T>>, ActionError<T>> {
        self.shared
            .stash
            .connection()
            .await?
            .tx(async |tx| {
                let mut res: Vec<QueuedActionOutput<T>> = vec![];

                for action in actions {
                    let meta = if let Some(last) = last_id {
                        Metadata::with_dependency(last)
                    } else {
                        Metadata::default()
                    };

                    let action = self
                        .queue_action_with_metadata_in_tx(action, meta, tx)
                        .await?;
                    last_id = Some(action.id);
                    res.push(action);
                }
                Ok(res)
            })
            .await
    }

    /// Queue an `action` for execution at a later time with a custom `metadata`.
    pub async fn queue_action_with_metadata<T: Action>(
        &self,
        action: T,
        metadata: Metadata,
    ) -> LocalOutput<T> {
        self.shared
            .stash
            .connection()
            .await?
            .tx(async |tx| {
                self.queue_action_with_metadata_in_tx(action, metadata, tx)
                    .await
            })
            .await
    }

    /// Queue an `action` for execution at a later time with a custom `metadata`.
    pub async fn queue_action_with_metadata_in_tx<T: Action>(
        &self,
        mut action: T,
        metadata: Metadata,
        tx: &Bond<'_>,
    ) -> LocalOutput<T> {
        let span = tracing::debug_span!("queue::queue", type=T::TYPE.0);

        async {
            debug!("Dependencies: {:?}", metadata.dependencies);

            let handler = self.shared.handler::<T>().ok_or_else(|| {
                error!("Tried to enqueue an unknown action: {}", T::TYPE);
                Error::UnknownAction(T::TYPE.to_string())
            })?;

            let (local_output, stored_action) =
                execute_action_local(&mut action, &handler, metadata, None, tx).await?;

            let id = stored_action.id.unwrap();

            info!("Action queued with id={id}");

            self.shared.queued_action_notifier.notify_waiters();
            let _ = self.shared.broadcast_sender.send(BroadcastMessage::Queued(
                id,
                Arc::new(QueuedMetadata::from(stored_action)),
            ));

            Ok(QueuedActionOutput {
                local: local_output,
                id,
            })
        }
        .instrument(span)
        .await
    }

    /// Attempt to replace an existing action with an updated version. If the action no longer
    /// exists or the types do not match, a new action will be queued instead.
    ///
    /// A default [`Metadata`] type is assigned to this `action`.
    pub async fn replace_or_queue_action<T: Action>(
        &self,
        existing_id: ActionId,
        action: T,
    ) -> LocalOutput<T> {
        self.replace_or_queue_action_with_metadata::<T>(existing_id, action, Metadata::default())
            .await
    }

    /// Attempt to replace an existing action with an updated version. If the action no longer
    /// exists or the types do not match, a new action will be queued instead.
    pub async fn replace_or_queue_action_with_metadata<T: Action>(
        &self,
        existing_id: ActionId,
        mut action: T,
        metadata: Metadata,
    ) -> LocalOutput<T> {
        let span = tracing::debug_span!("queue::replace", type=T::TYPE.0, existing_id=?existing_id);
        async {
            info!("Replacing {existing_id:?}");
            debug!("Dependencies: {:?}", metadata.dependencies);

            let handler = self.shared.handler::<T>().ok_or_else(|| {
                error!("Tried to enqueue an unknown action: {}", T::TYPE);
                Error::UnknownAction(T::TYPE.to_string())
            })?;

            let (local_output, stored_action) = self
                .shared
                .stash
                .connection()
                .await?
                .tx(async |tx| {
                    execute_action_local(&mut action, &handler, metadata, Some(existing_id), tx)
                        .await
                })
                .await?;
            let id = stored_action.id.unwrap();
            if existing_id == id {
                info!("Action has been updated");
                // We don't want to notify executors in this case.
            } else {
                info!("Action queued with id={id}");
                self.shared.queued_action_notifier.notify_waiters();
                let _ = self.shared.broadcast_sender.send(BroadcastMessage::Queued(
                    id,
                    Arc::new(QueuedMetadata::from(stored_action)),
                ));
            }

            Ok(QueuedActionOutput {
                local: local_output,
                id,
            })
        }
        .instrument(span)
        .await
    }

    /// Delete an action with `action_id` from the queue *without reverting local state*.
    ///
    /// To revert local state use [`Queue::cancel()`] or [`Queue::cancel_with_dependees()`].
    pub async fn delete_action(&self, action_id: ActionId) -> QueuedResult<()> {
        let mut tether = self.shared.stash.connection().await?;
        let existing_action_type = tether
            .tx(async |tx| {
                // Safety: It's safe to perform this check without an executor guard as sqlite's
                // single write transactions give us the freedom to safely validate this.
                if ExecutionGuard::has_executor(action_id, tx).await? {
                    return Err(QueuedError::ActionInExecution(action_id));
                }
                Ok(StoredAction::delete(tx, action_id).await?)
            })
            .await?;
        if let Some(existing_action_type) = existing_action_type {
            // Send only fails if there are no receivers, which is a valid state.
            let _ = self.shared.broadcast_sender.send(BroadcastMessage::Deleted(
                action_id,
                Arc::new(existing_action_type),
            ));
        }
        Ok(())
    }

    /// Returns the number of actions queued.
    pub async fn queued_actions_count(&self) -> Result<u64> {
        let tether = self.shared.stash.connection().await?;
        Ok(StoredAction::pending_count(&tether).await?)
    }

    pub async fn typed_actions_count<T: Action>(&self) -> Result<u64> {
        let tether = self.shared.stash.connection().await?;
        Ok(StoredAction::type_count::<T>(&tether).await?)
    }

    /// Check whether the action with `action_id` is present in the queue.
    pub async fn contains(&self, action_id: ActionId) -> Result<bool> {
        let tether = self.shared.stash.connection().await?;
        Ok(StoredAction::contains(&tether, action_id).await?)
    }

    /// Retrieve the metadata associated `action_id` in the queue.
    pub async fn action(&self, action_id: ActionId) -> Result<Option<QueuedMetadata>> {
        let tether = self.shared.stash.connection().await?;
        let stored_action = StoredAction::load(action_id, &tether).await?;
        Ok(stored_action.map(QueuedMetadata::from))
    }

    /// Deletes an action with `action_id` and allows the action to undo the local state. All other
    /// actions that depend on this action are also cancelled.
    ///
    /// To remove an action from the queue without reverting state see [`Queue::delete_action()`].
    pub async fn cancel(&self, action_id: ActionId) -> QueuedResult<Vec<ActionId>> {
        let mut tether = self.shared.stash.connection().await?;
        let cancelled_actions = tether
            .tx(async |tx| {
                // Safety: It's safe to perform this check without an executor guard as sqlite's
                // single write transactions give us the freedom to safely validate this.
                if ExecutionGuard::has_executor(action_id, tx).await? {
                    return Err(QueuedError::ActionInExecution(action_id));
                }
                cancel_action_with_dependees(&self.shared, tx, action_id).await
            })
            .await?;
        let cancelled_ids = cancelled_actions.iter().map(|v| v.id).collect();
        for cancelled_action in cancelled_actions {
            // Send only fails if there are no receivers, which is a valid state.
            let _ = self
                .shared
                .broadcast_sender
                .send(BroadcastMessage::Cancelled(cancelled_action));
        }
        Ok(cancelled_ids)
    }

    /// Retrieve the next action to execute.
    #[cfg(test)]
    pub(crate) async fn next_action(&self) -> Result<Option<StoredAction>, StashError> {
        let tether = self.shared.stash.connection().await?;
        StoredAction::next(ActionGroup::default().as_ref(), &tether).await
    }

    /// Create a new broadcast receiver to observe the state of queued actions.
    #[must_use]
    pub fn new_broadcast_receiver(&self) -> tokio::sync::broadcast::Receiver<BroadcastMessage> {
        self.shared.broadcast_sender.subscribe()
    }

    /// Create a new executor for this queue for a given `action_group`.
    #[must_use]
    pub fn new_executor_with_group(&self, action_group: ActionGroup) -> QueueExecutor {
        QueueExecutor::new(action_group, Arc::clone(&self.shared))
    }

    /// Create a new executor for this queue for the default action group.
    #[must_use]
    pub fn new_executor(&self) -> QueueExecutor {
        self.new_executor_with_group(ActionGroup::default())
    }

    /// Validate all pending actions can be loaded and deserialized to prevent
    /// infinite execution loops.
    pub async fn validate_queued_actions(&self) -> QueuedResult<()> {
        let tether = self.shared.stash.connection().await?;
        let actions = StoredAction::find("", vec![], &tether).await?;
        for action in actions {
            decode_action(&self.shared.factory, action)?;
        }
        Ok(())
    }

    #[cfg(feature = "rebase")]
    pub async fn rebase(
        &self,
        action_group: ActionGroup,
        change_set: &RebaseChangeSet,
    ) -> QueuedResult<()> {
        let mut tether = self.shared.stash.connection().await?;
        tether
            .tx(async |tx| self.rebase_in(action_group, change_set, tx).await)
            .await
    }

    #[cfg(feature = "rebase")]
    pub async fn rebase_in(
        &self,
        action_group: ActionGroup,
        change_set: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> QueuedResult<()> {
        let ids = StoredAction::rebase_action_order(action_group.as_ref(), tx).await?;
        if ids.is_empty() {
            return Ok(());
        }

        for id in ids {
            let action = StoredAction::load(id, tx)
                .await?
                .ok_or(QueuedError::ActionNotFound(id))?;
            let (mut decoded, meta) = decode_action(&self.shared.factory, action.clone())?;
            decoded.rebase(action, meta, change_set, tx).await?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum QueuedActionState {
    Executed(ActionId),
    Queued(ActionId, ActionRequeueReason),
}

#[derive(Debug, Clone, Copy)]
pub enum ActionRequeueReason {
    /// There was an intermittent networking issue - the action should be
    /// restarted later.
    NetworkFailed,

    /// Another executor was already working on this action, but then died or
    /// dead-locked mid-execution - the action should be restarted later.
    GuardExpired,

    /// Action's handler needs access to some external data that is now gone
    /// (e.g. it has a `Weak` pointer that can't be upgraded anymore) - the
    /// action should be restarted later.
    LostContext,
}

pub(crate) trait ErasedQueuedAction: Send {
    fn execute<'a>(
        &'a mut self,
        shared: &'a Shared,
        tether: &'a mut Tether,
        guard: ExecutionGuard,
        metadata: Arc<QueuedMetadata>,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<QueuedActionState>> + 'a + Send>>;

    fn cancel<'a>(
        &'a mut self,
        tx: &'a Bond,
        metadata: Arc<QueuedMetadata>,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>>;

    #[cfg_attr(not(feature = "rebase"), allow(dead_code))]
    fn rebase<'a>(
        &'a mut self,
        action: StoredAction,
        metadata: Arc<QueuedMetadata>,
        change_set: &'a RebaseChangeSet,
        tx: &'a Bond,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>>;
}

pub(crate) struct QueuedAction<T: Action + Send> {
    pub id: ActionId,
    pub action: T,
    pub handler: Arc<T::Handler>,
}

impl<T: Action> ErasedQueuedAction for QueuedAction<T> {
    fn execute<'a>(
        &'a mut self,
        shared: &'a Shared,
        tether: &'a mut Tether,
        guard: ExecutionGuard,
        metadata: Arc<QueuedMetadata>,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<QueuedActionState>> + 'a + Send>> {
        Box::pin(async move {
            let output = execute_action_remote(
                shared,
                self.id,
                &*self.handler,
                &mut self.action,
                tether,
                guard,
            )
            .await
            .map_err(|e| QueuedError::Action(Arc::new(anyhow::Error::new(e)), metadata))?;

            Ok(match output {
                ActionRemoteOutput::Executed(_) => QueuedActionState::Executed(self.id),
                ActionRemoteOutput::Queued(id, reason) => QueuedActionState::Queued(id, reason),
            })
        })
    }

    fn cancel<'a>(
        &'a mut self,
        tx: &'a Bond,
        metadata: Arc<QueuedMetadata>,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>> {
        let span = tracing::debug_span!("queue::revert", id=self.id.0, type=T::TYPE.0);

        Box::pin(
            async move {
                info!("Reverting local state");

                if let Err(e) = self
                    .handler
                    .revert_local(self.id, &mut self.action, tx)
                    .await
                {
                    error!("Failed to revert local changes: {e:?}");
                }

                StoredAction::delete(tx, self.id)
                    .await
                    .map_err(|e| {
                        error!("Failed to delete action: {e:?}");
                        e
                    })
                    .map_err(|e| QueuedError::Action(Arc::new(anyhow::Error::new(e)), metadata))?;

                Ok(())
            }
            .instrument(span),
        )
    }

    fn rebase<'a>(
        &'a mut self,
        mut action: StoredAction,
        metadata: Arc<QueuedMetadata>,
        change_set: &'a RebaseChangeSet,
        tx: &'a Bond,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>> {
        let span = tracing::debug_span!("queue::rebase", id=self.id.0, type=T::TYPE.0);
        Box::pin(
            async move {
                tracing::info!("Rebasing local state");
                self.handler
                    .rebase_local(self.id, &mut self.action, change_set, tx)
                    .await
                    .map_err(|e| {
                        error!("Failed to rebase local changes: {e:?}");
                        QueuedError::Action(Arc::new(anyhow::Error::new(e)), metadata)
                    })?;

                action
                    .set_action_state(&self.action)
                    .map_err(|e| StashError::Custom(anyhow::Error::new(e)))?;
                action.save(tx).await?;
                Ok(())
            }
            .instrument(span),
        )
    }
}

/// A Queue Executor which can pop actions from the [`Queue`] and execute them.
///
/// Many executors can be assigned to a queue based on given [`ActionGroup`]. You can
/// create one with [`Queue::new_executor()`].
pub struct QueueExecutor {
    shared: Arc<Shared>,
    action_group: ActionGroup,
    id: String,
}

impl Drop for QueueExecutor {
    fn drop(&mut self) {
        tracing::info!(?self.id, "Dropping QueueExecutor");
    }
}

impl QueueExecutor {
    fn new(action_group: ActionGroup, shared: Arc<Shared>) -> Self {
        Self {
            action_group,
            shared,
            id: Uuid::new_v4().to_string(),
        }
    }

    /// Return's the unique id of this executor.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Convert this executor into a [`QueueAutoExecutor`] with the default termination policy
    #[must_use]
    pub fn into_auto_executor(
        self,
        online: Box<dyn OnlineStatusWaiter>,
        start_paused: bool,
        task_spawner: &impl TaskSpawner,
        span: tracing::Span,
    ) -> QueueAutoExecutor {
        self.into_auto_executor_with_policy(
            online,
            start_paused,
            task_spawner,
            QueueAutoTerminationPolicy::Never,
            span,
        )
    }

    /// Convert this executor into a [`QueueAutoExecutor`] with a custom termination policy
    #[must_use]
    pub fn into_auto_executor_with_policy(
        self,
        online: Box<dyn OnlineStatusWaiter>,
        start_paused: bool,
        task_spawner: &impl TaskSpawner,
        termination_policy: QueueAutoTerminationPolicy,
        span: tracing::Span,
    ) -> QueueAutoExecutor {
        QueueAutoExecutor::new(
            self,
            online,
            start_paused,
            task_spawner,
            termination_policy,
            span,
        )
    }

    /// Execute one action from the queue.
    pub async fn execute_one(&self) -> QueuedResult<Option<QueuedActionState>> {
        let mut tether = self.shared.stash.connection().await?;
        self.execute_impl(&mut tether).await
    }

    /// Execute all available actions from the queue.
    ///
    /// Returns the number of executed actions.
    pub async fn execute_all(&self) -> QueuedResult<usize> {
        let mut tether = self.shared.stash.connection().await?;
        let mut counter = 0;
        while let Some(QueuedActionState::Executed(_)) = self.execute_impl(&mut tether).await? {
            counter += 1;
        }
        Ok(counter)
    }

    /// Load the next action and execute it.
    ///
    /// If no action is found, this method returns `None`. Otherwise, we
    /// return the id of the executed action.
    async fn execute_impl(&self, tether: &mut Tether) -> QueuedResult<Option<QueuedActionState>> {
        let Some((exec_guard, action)) = self.next_action(tether).await.map_err(|e| {
            error!("Failed to retrieve action: {e:?}");
            e
        })?
        else {
            return Ok(None);
        };

        let action_id = action.id.unwrap();
        let action_type = action.action_type.clone();
        let debug_span = tracing::debug_span!("queue::execute",id=action_id.0, type=action_type);

        async {
            info!("Executing action");
            debug!("{}", action.short_dbg_str());
            let (mut decoded, metadata) = match decode_action(&self.shared.factory, action) {
                Ok(v) => v,
                Err(e) => {
                    // Release execution guard if decode failed.
                    {
                        if let Err(e) = async {
                            tether.tx(async |tx| exec_guard.release(tx).await).await?;
                            Ok::<_, StashError>(())
                        }
                        .await
                        {
                            error!("Failed to release execution guard after decode failed: {e:?}");
                        }
                    }
                    return Err(e);
                }
            };

            let exec_output = decoded
                .execute(&self.shared, tether, exec_guard, metadata.clone())
                .await
                .inspect_err(|e| {
                    if let QueuedError::Action(err, metadata) = e {
                        // Send only fails if there are no receivers, which is a valid state.
                        let _ = self.shared.broadcast_sender.send(BroadcastMessage::Error(
                            Arc::clone(err),
                            Arc::clone(metadata),
                        ));
                    }
                })?;

            // Send only fails if there are no receivers, which is a valid state.
            let _ = self
                .shared
                .broadcast_sender
                .send(BroadcastMessage::Success(action_id, metadata));

            Ok(Some(exec_output))
        }
        .instrument(debug_span)
        .await
    }

    async fn next_action(
        &self,
        tether: &mut Tether,
    ) -> Result<Option<(ExecutionGuard, StoredAction)>, StashError> {
        StoredAction::pop(self.id.clone(), self.action_group.as_ref(), tether).await
    }
}

/// Control the behavior by why the executor will terminate
#[derive(Debug, Copy, Clone)]
pub struct QueueAutoTerminationPolicy(u8);

bitflags! {
    impl QueueAutoTerminationPolicy: u8 {
        /// Run forever and ever and even...
        const Never=0;
        /// Stop executing as soon as the queue is empty. Note that is can cause
        /// early exists with concurrent modifications of the queue.
        const Empty=1<<0;
        /// Stop executing when we detect there is not network.
        const NetworkLoss=1<<1;
        /// Combines both [`Empty`] and [`NetworkLoss`] behaviors.
        const EmptyOrNetworkLoss = QueueAutoTerminationPolicy::Empty.bits() | QueueAutoTerminationPolicy::NetworkLoss.bits();
    }
}

impl QueueAutoTerminationPolicy {
    fn is_empty_policy(self) -> bool {
        self.intersects(QueueAutoTerminationPolicy::Empty)
    }

    fn is_network_loss_policy(self) -> bool {
        self.intersects(QueueAutoTerminationPolicy::NetworkLoss)
    }
}

pub trait TaskSpawner {
    fn spawn_task<F>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = ()> + Send + 'static;
}

#[async_trait::async_trait]
pub trait OnlineStatusWaiter: Send {
    async fn wait_until_online(&mut self);
}

pub trait OnlineStatusWaiterBuilder {
    fn build(&self) -> Box<dyn OnlineStatusWaiter>;
}

pub struct NoopOnlineStatusWaiter;

#[async_trait::async_trait]
impl OnlineStatusWaiter for NoopOnlineStatusWaiter {
    async fn wait_until_online(&mut self) {}
}

pub struct NoopOnlineStatusWaiterBuilder;

impl OnlineStatusWaiterBuilder for NoopOnlineStatusWaiterBuilder {
    fn build(&self) -> Box<dyn OnlineStatusWaiter> {
        Box::new(NoopOnlineStatusWaiter)
    }
}

pub struct TokioTaskSpawner;

impl TaskSpawner for TokioTaskSpawner {
    fn spawn_task<F>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        tokio::spawn(future)
    }
}

/// This executor will automatically execute action from the [`Queue`] as soon as they are inserted.
///
/// When executing in the same process, the executor can react very quickly to actions that
/// are added to the queue.
///
/// In a cross-process setting we currently rely on a timeout to ensure that actions queued
/// by another process are executed.
pub struct QueueAutoExecutor {
    task: JoinHandle<()>,
    id: String,
    paused: watch::Sender<bool>,
}

impl Drop for QueueAutoExecutor {
    fn drop(&mut self) {
        self.terminate();
    }
}

impl QueueAutoExecutor {
    fn new(
        executor: QueueExecutor,
        online: Box<dyn OnlineStatusWaiter>,
        start_paused: bool,
        task_spawner: &impl TaskSpawner,
        termination_policy: QueueAutoTerminationPolicy,
        span: tracing::Span,
    ) -> Self {
        let id = executor.id.clone();
        let (paused_tx, paused_rx) = watch::channel(start_paused);

        let task = task_spawner.spawn_task(async move {
            Self::run(executor, paused_rx, online, termination_policy)
                .instrument(span)
                .await;
        });

        QueueAutoExecutor {
            task,
            id,
            paused: paused_tx,
        }
    }

    /// Return's the unique id of this executor.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Pause auto execution.
    ///
    /// When executor is currently running it will pause before picking another task.
    /// It will be paused until `resume` is called.
    ///
    pub fn pause(&self) {
        self.paused.send_replace(true);
    }

    /// Resume auto execution.
    ///
    /// It will have an effect only if executor was paused before calling `resume`.
    /// The execution will be resumed.
    ///
    pub fn resume(&self) {
        self.paused.send_replace(false);
    }

    async fn run(
        executor: QueueExecutor,
        mut paused: watch::Receiver<bool>,
        mut online: Box<dyn OnlineStatusWaiter>,
        termination_policy: QueueAutoTerminationPolicy,
    ) {
        debug!(
            "Starting auto queue executor {} with group={}",
            executor.id, executor.action_group
        );

        loop {
            if *paused.borrow() {
                let eid = executor.id();

                debug!(?eid, "Pausing executor");
                _ = paused.wait_for(|paused| !paused).await;
                debug!(?eid, "Resuming executor");
            }

            let followup = match executor.execute_one().await {
                Ok(None) => ActionExecutionFollowup::WaitForAction,
                Ok(Some(QueuedActionState::Queued(_, ActionRequeueReason::LostContext))) => {
                    ActionExecutionFollowup::WaitForAction
                }
                Ok(Some(QueuedActionState::Queued(_, ActionRequeueReason::NetworkFailed))) => {
                    if termination_policy.is_network_loss_policy() {
                        return;
                    }
                    ActionExecutionFollowup::WaitForNetwork
                }
                Ok(Some(QueuedActionState::Executed(_))) => ActionExecutionFollowup::PickNextAction,
                Ok(Some(QueuedActionState::Queued(_, _))) => {
                    ActionExecutionFollowup::PickNextAction
                }
                Err(e) => {
                    error!("Failed to execute action: {e}");
                    ActionExecutionFollowup::PickNextAction
                }
            };

            match followup {
                ActionExecutionFollowup::WaitForAction => {
                    if termination_policy.is_empty_policy() {
                        let Ok(tether) = executor.shared.stash.connection().await else {
                            tracing::error!("Failed to acquire db connection");
                            continue;
                        };
                        if let Ok(count) =
                            StoredAction::pending_count(&tether).await.inspect_err(|e| {
                                error!("Failed to get pending action count: {e:?}");
                            })
                            && count == 0
                        {
                            return;
                        }
                    }
                    // We currently wait for a signal from an action queue to start executing.
                    // The timeout is here to catch potential changes made in another process.
                    // This can be revisited once we have a cross process database observer.
                    let _ = tokio::time::timeout(
                        DEFAULT_LOCK_TIMEOUT,
                        executor.shared.queued_action_notifier.notified(),
                    )
                    .await;
                }

                ActionExecutionFollowup::WaitForNetwork => {
                    debug!("Waiting for network connection");
                    online.wait_until_online().await;
                    debug!("Network connection restored - resuming the auto queue executor");
                }

                ActionExecutionFollowup::PickNextAction => (),
            }
        }
    }

    /// Terminate the execution of actions.
    pub fn terminate(&self) {
        self.task.abort();
    }

    /// Wait on the executor to finish.
    pub async fn await_finished(mut self) {
        let _ = (&mut self.task).await;
    }
}

#[derive(Clone, Copy)]
enum ActionExecutionFollowup {
    WaitForAction,
    WaitForNetwork,
    PickNextAction,
}

pub struct QueueAutoExecutorPool {
    executors: Vec<QueueAutoExecutor>,
}

impl QueueAutoExecutorPool {
    #[must_use]
    pub fn new(
        queue: &Queue,
        action_group: &ActionGroup,
        count: NonZeroUsize,
        online: &impl OnlineStatusWaiterBuilder,
        start_paused: bool,
        task_spawner: &impl TaskSpawner,
        span: tracing::Span,
    ) -> Self {
        Self::with_termination_policy(
            queue,
            action_group,
            count,
            online,
            start_paused,
            task_spawner,
            QueueAutoTerminationPolicy::Never,
            span,
        )
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn with_termination_policy(
        queue: &Queue,
        action_group: &ActionGroup,
        count: NonZeroUsize,
        online: &impl OnlineStatusWaiterBuilder,
        start_paused: bool,
        task_spawner: &impl TaskSpawner,
        termination_policy: QueueAutoTerminationPolicy,
        span: tracing::Span,
    ) -> Self {
        let executors = (0..count.get())
            .map(move |_| {
                queue
                    .new_executor_with_group(action_group.clone())
                    .into_auto_executor_with_policy(
                        online.build(),
                        start_paused,
                        task_spawner,
                        termination_policy,
                        span.clone(),
                    )
            })
            .collect::<Vec<_>>();

        Self { executors }
    }

    pub fn terminate(&self) {
        for executor in &self.executors {
            executor.terminate();
        }
    }

    pub fn pause(&self) {
        for executor in &self.executors {
            executor.pause();
        }
    }

    pub fn resume(&self) {
        for executor in &self.executors {
            executor.resume();
        }
    }

    pub async fn await_finished(self) {
        for executor in self.executors {
            executor.await_finished().await;
        }
    }
}

async fn execute_action_local<T: Action>(
    action: &mut T,
    handler: &T::Handler,
    metadata: Metadata,
    existing_id: Option<ActionId>,
    tx: &Bond<'_>,
) -> Result<(T::LocalOutput, StoredAction), ActionError<T>> {
    let mut stored_action = StoredAction::without_state::<T>(action.dependency_keys(), metadata);
    if let Some(exising_id) = existing_id {
        stored_action
            .create_or_update(exising_id, tx)
            .await
            .map_err(|e| {
                error!("Failed to create or update action: {e:?}");
                e
            })?;
    } else {
        stored_action.save(tx).await.map_err(|e| {
            error!("Failed to store action: {e:?}");
            e
        })?;
    }

    // Validate action dependencies for circular deps
    {
        let mut sorter = TopologicalSort::<ActionId>::new();
        let mut pending_action_ids = vec![stored_action.id.unwrap()];
        let mut visited = HashSet::new();
        while let Some(action_id) = pending_action_ids.pop() {
            let deps = StoredAction::all_dependencies(tx, action_id).await?;
            if !visited.insert(action_id) {
                continue;
            }
            if deps.is_empty() {
                continue;
            }
            for dep in &deps {
                sorter.add_dependency(action_id, dep.dependency_id);
            }
            pending_action_ids.extend(deps.into_iter().map(|dep| dep.dependency_id));
        }
        if sorter.pop().is_none() && !sorter.is_empty() {
            return Err(Error::CyclicDependency.into());
        }
    }

    // Execute the local changes
    let local_output = handler
        .apply_local(stored_action.id.unwrap(), action, tx)
        .await
        .map_err(|e| {
            error!("Failed to apply local changes: {e:?}");
            ActionError::Action(e)
        })?;

    // Update action state.
    stored_action.set_action_state(action).map_err(|e| {
        error!("Failed to set action state: {e:?}");
        Error::from(e)
    })?;
    stored_action
        .update_action_state(tx)
        .await
        .inspect_err(|e| {
            error!("Failed to update action state: {e:?}");
        })?;

    Ok((local_output, stored_action))
}

async fn execute_action_remote<T: Action>(
    shared: &Shared,
    id: ActionId,
    handler: &T::Handler,
    action: &mut T,
    tether: &mut Tether,
    guard: ExecutionGuard,
) -> Result<ActionRemoteOutput<T::RemoteOutput>, ActionError<T>> {
    debug!("Applying action on remote");

    let writer_guard = WriterGuard::new(tether, &guard);
    let result = handler.apply_remote(id, action, writer_guard).await;
    let mut cancelled_actions = vec![];

    let result = match guard
        .tx_and_release(tether, async |tx| {
            let result = async {
                match result {
                    Ok(result) => {
                        StoredAction::delete(tx, id).await?;

                        info!("Action executed");

                        Ok(ActionRemoteOutput::Executed(result))
                    }

                    Err(e) => {
                        error!("Failed to apply on server: {e:?}");

                        if let Some(reason) = e.can_requeue() {
                            let retries = StoredAction::get_retries(tx, id).await?;
                            if let Some(max_retries) = T::MAX_RETRIES
                                && retries >= max_retries
                            {
                                debug!(
                                    ?reason,
                                    "Action has reached max retries and will be cancelled"
                                );
                            } else {
                                debug!(?reason, "Action will be requeued");
                                StoredAction::update_retries(tx, id).await?;

                                return Ok(ActionRemoteOutput::Queued(id, reason));
                            }
                        }

                        match cancel_action_with_dependees(shared, tx, id).await {
                            Ok(ids) => {
                                cancelled_actions = ids;
                            }
                            Err(e) => {
                                error!("Failed to cancel action and depeendees: {e:?}");
                            }
                        }

                        info!("Action Reverted");
                        Err(ActionError::Action(e))
                    }
                }
            }
            .await;

            Ok(result)
        })
        .await
    {
        Ok(v) => v,
        Err(WriterGuardError::Expired) => {
            return Ok(ActionRemoteOutput::Queued(
                id,
                ActionRequeueReason::GuardExpired,
            ));
        }
        Err(WriterGuardError::Stash(e)) => return Err(e.into()),
    };

    for cancelled_action in cancelled_actions.into_iter().filter(|meta| {
        // We don't want to report cancellation of our own action, only of the dependees.
        meta.id != id
    }) {
        // Send only fails if there are no receivers, which is a valid state.
        let _ = shared
            .broadcast_sender
            .send(BroadcastMessage::Cancelled(cancelled_action));
    }

    result
}

async fn cancel_action_with_dependees(
    shared: &Shared,
    bond: &Bond<'_>,
    action_id: ActionId,
) -> QueuedResult<Vec<Arc<QueuedMetadata>>> {
    let mut remaining_actions = vec![action_id];
    let mut sorter = TopologicalSort::<ActionId>::new();
    let mut cancelled_actions = Vec::new();

    while let Some(action_id) = remaining_actions.pop() {
        let dependees = StoredAction::dependees_of_type(bond, action_id, DependencyType::Required)
            .await
            .map_err(|e| {
                error!("Failed to load action dependees: {e:?}");
                e
            })?;

        debug!("Action {action_id} has {:?} as dependees", dependees);
        remaining_actions.extend(dependees.iter().copied());

        for id in dependees {
            sorter.add_dependency(id, action_id);
        }
    }

    if sorter.is_empty() {
        // This means that the current action has no dependency chain
        // we should only revert this action.
        let Some(action) = StoredAction::load(action_id, bond).await? else {
            return Err(QueuedError::ActionNotFound(action_id));
        };

        let (mut decoded, metadata) = decode_action(&shared.factory, action)?;

        decoded.cancel(bond, Arc::clone(&metadata)).await?;
        cancelled_actions.push(metadata);
    } else {
        debug!("Reverting {} dependent actions", sorter.len());

        // Cancel all actions in reversed order
        while let Some(current_action_id) = sorter.pop() {
            let Some(action) = StoredAction::load(current_action_id, bond).await? else {
                return Err(QueuedError::ActionNotFound(current_action_id));
            };

            let (mut decoded, metadata) = decode_action(&shared.factory, action)?;

            decoded.cancel(bond, Arc::clone(&metadata)).await?;
            cancelled_actions.push(metadata);
        }
    }

    Ok(cancelled_actions)
}

fn decode_action(
    factory: &RwLock<Factory>,
    stored_action: StoredAction,
) -> QueuedResult<(Box<dyn ErasedQueuedAction>, Arc<QueuedMetadata>)> {
    let action_id = stored_action.id.unwrap();

    let action_type = stored_action.action_type.clone();
    factory.read().decode(stored_action).map_err(|e| {
        error!(?action_type, "Failed to decode action: {e:?}");
        QueuedError::Factory(action_id, e)
    })
}

#[macro_export]
/// Enqueues actions of potentially different types directly with the queue.
///
/// Example usage:
/// `let action_id = enqueue!(my_queue, [foo, bar, baz])?;`
macro_rules! enqueue {
    ($queue:expr, [$($action:expr),+ $(,)?]) => {{
        use $crate::queue::{Queue, MultiActionError};
        use $crate::action::{ActionId, Metadata};
        use ::anyhow::anyhow;

        $queue.tether().await?.tx::<_,_, MultiActionError>(async |tx| {
            let mut last = None;
            $(
                let meta = if let Some(last) = last {
                    Metadata::with_dependency(last)
                } else {
                    Metadata::default()
                };
                let action = $queue
                    .queue_action_with_metadata_in_tx($action, meta, tx)
                    .await?;
                last = Some(action.id);
            )+
            // This is safe to do because we'd short circuit if this would be None, and this requires
            // 1+ params.
            Ok(last.unwrap())
        }).await
    }}
}
