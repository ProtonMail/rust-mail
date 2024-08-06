#[cfg(test)]
#[path = "tests/queue.rs"]
mod tests;

use crate::action::{
    Action, Error as ActionErrorTrait, Factory, FactoryError, FactoryResult, Handler, Id, Metadata,
    Priority,
};
use crate::db::{self, StoredAction};
use chrono::DateTime;
use parking_lot::RwLock;
use proton_api_core::session::Session;
use proton_sqlite3::MigratorError;
use stash::stash::{Stash, StashError, Tether};
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use topological_sort::TopologicalSort;
use tracing::{debug, error, Level};

/// Errors which can occur while operating on the queue.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Migration error: {0}")]
    Migrator(#[from] MigratorError),
    #[error("DB Error: {0}")]
    DB(#[from] StashError),
    #[error("Serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),
    #[error("Deserialization error: {0}")]
    Deserialization(#[from] rmp_serde::decode::Error),
    #[error("Action {0} was cancelled")]
    Cancelled(Id),
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

// Custom debug impl, otherwise T also needs to have Debug when it is not really necessary.
impl<T: Action> Debug for ActionError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionError::Action(err) => {
                write!(f, "ActionError::Action{{{err:?}}}")
            }
            ActionError::Queue(err) => {
                write!(f, "ActionError::Queue{{{err:?}}}")
            }
        }
    }
}

impl<T: Action> From<StashError> for ActionError<T> {
    fn from(value: StashError) -> Self {
        Self::Queue(value.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Errors that may arise from executing queued actions.
#[derive(Debug, thiserror::Error)]
pub enum QueuedError {
    #[error("Factory Error (ActionId={0}): {1}")]
    Factory(Id, FactoryError),
    #[error("Queued Action error: {0}")]
    Action(anyhow::Error, Box<QueuedMetadata>),
    #[error("DB Error: {0}")]
    DB(#[from] StashError),
    #[error("Action {0} does not exist")]
    ActionNotFound(Id),
    #[error("Another thread is currently operating the queue")]
    Busy,
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

impl QueuedError {
    /// If queued action failed you can attempt to retrieve the error of the action via this
    /// function.
    ///
    /// If the action type does not match or the error type does not match, `None` is returned.
    #[must_use]
    pub fn action_error<T: Action>(&self) -> Option<&ActionError<T>> {
        let Self::Action(err, _) = self else {
            return None;
        };

        err.as_action_error::<T>()
    }
}
pub type QueuedResult<T> = std::result::Result<T, QueuedError>;

/// Metadata associated with a queued [`Action`].
#[derive(Debug)]
pub struct QueuedMetadata {
    /// Identifier of the stored action.
    pub id: Id,
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
    pub dependencies: Vec<Id>,
    /// Optional debug string associated with this action.
    pub debug_string: Option<String>,
    /// Resources which were associated with this action.
    pub resources: Vec<Vec<u8>>,
}

impl From<StoredAction> for QueuedMetadata {
    fn from(value: StoredAction) -> Self {
        Self {
            id: value.id,
            action_type: value.action_type,
            version: value.version,
            created: value.created,
            scheduled: value.scheduled,
            priority: value.priority,
            dependencies: value.dependencies,
            debug_string: value.debug_string,
            resources: value.resources,
        }
    }
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
/// There are two modes of execution immediate and queued.
///
/// # Immediate Mode
///
/// As the name indicated, immediate mode attempts to execute the actions immediately. However,
/// if we detect that we can't apply the action on the remote due to lack of network, the action
/// is automatically queued.
///
/// If the action can complete all steps successfully [`Action::Output`] is returned as part of
/// the result.
///
/// If the action fails, [`Action::Error`] is returned with [`ActionError`].
///
/// See:
/// * [`Queue::apply_action()`]
/// * [`Queue::apply_action_with_metadata()`]
///
///
/// # Queued Mode
///
/// In this mode the local changes are applied first and the action is queued for execution as
/// soon as all the conditions are met (Priority, delay and/or dependencies). Queued actions are
/// persisted into the database until they are executed, cancelled or deleted.
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
/// The queue does not automatically execute actions that are queued. The integrator needs to decide
/// when the right moment is for the queue operate.
///
/// Note that if an action can't be executed due to network issues, no error is returned and
/// it will be retried on the next invocation.
///
/// See:
/// * [`Queue::execute_one()`]
/// * [`Queue::execute_all()`]
/// * [`Queue::execute_with_limit()`]
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
/// are currently not supported.
///
/// ## Concurrency
///
/// The queue supports queuing actions from multiple threads, but modifying the queue (e.g.:
/// deleting or executing actions) is guarded so that only the operation can be executed in
/// isolation. If more than one location attempts to call these functions currently we
/// we will return [`QueuedError::Busy`].
///

pub struct Queue {
    stash: Stash,
    factory: RwLock<Factory>,
    exec_guard: AtomicBool,
}

/// State of the [`Action`] after being applied with [`Queue::apply_action`] or
/// [`Queue::apply_action_with_metadata`].
pub enum ActionStatus<T> {
    /// Action was executed successfully on local and on remote.
    Executed(T),
    /// Action could not be executed on the remote at this time and was queued.
    Queued(Id),
}

impl Queue {
    /// Create a new queue with the given `stash`;
    ///
    /// # Errors
    ///
    /// Returns error if the database migration failed.
    pub async fn new(stash: Stash) -> Result<Self> {
        Self::with_factory(stash, Factory::default()).await
    }

    /// Create a new queue with the given `stash` and `factory`;
    ///
    /// # Errors
    ///
    /// Returns error if the database migration failed.
    pub async fn with_factory(stash: Stash, factory: Factory) -> Result<Self> {
        db::create_tables(&stash).await?;
        Ok(Self {
            stash,
            factory: RwLock::new(factory),
            exec_guard: AtomicBool::new(false),
        })
    }

    /// Register an [`Action`] with the factory.
    ///
    /// # Errors
    ///
    /// Returns error if the action type was already registered before.
    pub fn register<T: Action + 'static>(&self) -> FactoryResult<()> {
        self.factory.write().register::<T>()
    }

    /// Return the database associated with the queue.
    #[must_use]
    pub fn stash(&self) -> &Stash {
        &self.stash
    }

    /// Queue an `action` for execution at a later time.
    ///
    /// A default [`Metadata`] type is assigned to this `action`.
    ///
    /// # Errors
    ///
    /// Returns error if action could not be executed locally.
    pub async fn queue_action<T: Action>(
        &self,
        action: T,
    ) -> std::result::Result<Id, ActionError<T>> {
        self.queue_action_with_metadata::<T>(action, Metadata::default())
            .await
    }

    /// Queue an `action` for execution at a later time with a custom `metadata`.
    ///
    /// # Errors
    ///
    /// Returns error if action could not be executed locally.
    #[tracing::instrument(level = Level::DEBUG, skip(self, metadata, action), name =
    "QueueAction")]
    pub async fn queue_action_with_metadata<T: Action>(
        &self,
        mut action: T,
        metadata: Metadata,
    ) -> std::result::Result<Id, ActionError<T>> {
        debug!("Queueing action: {} {:?}", T::TYPE, metadata,);
        let handler = T::Handler::default();

        let id = self
            .execute_action_local(&handler, &mut action, metadata)
            .await?;
        debug!("Action queued with id={id}");
        Ok(id)
    }

    /// Execute an `action` immediately.
    ///
    /// A default [`Metadata`] type is assigned to this `action`.
    ///
    /// The action will only be queued if the remote fails with a network error.
    ///
    /// # Remarks
    ///
    /// Note that the `metadata` type is only used if the action is queued.
    ///
    /// # Errors
    ///
    /// Returns error if action could not be executed locally or remotely.
    pub async fn apply_action<T: Action>(
        &self,
        session: &Session,
        action: T,
    ) -> std::result::Result<ActionStatus<T::Output>, ActionError<T>> {
        self.apply_action_with_metadata::<T>(session, action, Metadata::default())
            .await
    }

    /// Execute an `action` immediately with a custom `metadata`.
    ///
    /// A default [`Metadata`] type is assigned to this `action`.
    ///
    /// The action will only be queued if the remote fails with a network error.
    ///
    /// # Remarks
    ///
    /// Note that the `metadata` type is only used if the action is queued.
    ///
    /// # Errors
    ///
    /// Returns error if action could not be executed locally or remotely.
    #[tracing::instrument(level = Level::DEBUG, skip(self, session, metadata, action), name =
    "ApplyAction")]
    pub async fn apply_action_with_metadata<T: Action>(
        &self,
        session: &Session,
        mut action: T,
        metadata: Metadata,
    ) -> std::result::Result<ActionStatus<T::Output>, ActionError<T>> {
        debug!("Applying action: {} {:?}", T::TYPE, metadata,);
        let handler = T::Handler::default();

        // 1) Apply local action and store in the queue
        let id = self
            .execute_action_local(&handler, &mut action, metadata)
            .await?;
        debug!("Action queued with id={id}");

        // 2) Execute remote counter part
        self.execute_action_remote(id, &handler, &mut action, session)
            .await
    }

    /// Execute one action from the queue, if available, using the given `session` for remote
    /// communication.
    ///
    /// # Errors
    ///
    /// Returns error if the queued action could not be executed locally or remotely, or if
    /// another thread is currently invoking this function.
    pub async fn execute_one(&self, session: &Session) -> QueuedResult<Option<Id>> {
        self.with_exec_guard(async { self.execute_impl(session).await })
            .await
    }

    /// Execute all available actions from the queue, using the given `session` for
    /// remote communication.
    ///
    /// # Errors
    ///
    /// Returns error if the queued action could not be executed locally or remotely, or if
    /// another thread is currently invoking this function.
    pub async fn execute_all(&self, session: &Session) -> QueuedResult<()> {
        self.with_exec_guard(async {
            while self.execute_impl(session).await?.is_some() {}
            Ok(())
        })
        .await
    }

    /// Execute up to `limit` available actions from the queue, using the given `session` for
    /// remote communication.
    ///
    /// # Errors
    ///
    /// Returns error if the queued action could not be executed locally or remotely, or
    /// if another thread is currently invoking this function.
    pub async fn execute_with_limit(&self, session: &Session, limit: usize) -> QueuedResult<()> {
        self.with_exec_guard(async {
            for _ in 0..limit {
                if self.execute_impl(session).await?.is_none() {
                    return Ok(());
                }
            }

            Ok(())
        })
        .await
    }

    /// Delete an action with `action_id` from the queue *without reverting local state*.
    ///
    /// To revert local state use [`Queue::cancel()`] or [`Queue::cancel_with_dependees()`].
    ///
    /// # Errors
    ///
    /// Returns error if the db operation failed or if another thread is currently invoking
    /// this function.
    pub async fn delete_action(&self, action_id: Id) -> QueuedResult<()> {
        self.with_exec_guard(async {
            let tx = self.stash.transaction().await?;
            StoredAction::delete(&tx, action_id).await?;
            tx.commit().await?;
            Ok(())
        })
        .await
    }

    /// Returns the number of actions queued.
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed.
    pub async fn queued_actions_count(&self) -> Result<usize> {
        let tether = self.stash.connection();
        Ok(StoredAction::pending_count(&tether).await?)
    }

    /// Check whether the action with `action_id` is present in the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed.
    pub async fn contains(&self, action_id: Id) -> Result<bool> {
        let tether = self.stash.connection();
        Ok(StoredAction::contains(&tether, action_id).await?)
    }

    /// Retrieve the metadata associated `action_id` in the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed.
    pub async fn action(&self, action_id: Id) -> Result<Option<QueuedMetadata>> {
        let tether = self.stash.connection();
        let stored_action = StoredAction::with_id(&tether, action_id).await?;
        Ok(stored_action.map(QueuedMetadata::from))
    }

    /// Deletes an action with `action_id` and allows the action to undo the local state.
    ///
    /// To remove an action from the queue without reverting state see [`Queue::delete_action()`].
    ///
    /// To cancel this action and all the actions that depend on it see
    /// [`Queue::cancel_with_dependees()`].
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed, the action could not be found or another thread
    /// is currently invoking this function.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn cancel(&self, action_id: Id) -> QueuedResult<()> {
        let conn = self.stash.connection();
        self.with_exec_guard(async move {
            let Some(action) = StoredAction::with_id(&conn, action_id).await? else {
                return Err(QueuedError::ActionNotFound(action_id));
            };

            let (mut decoded, metadata) = self.decode_action(action)?;
            conn.transaction().await?;
            decoded.cancel(&conn, metadata).await?;
            conn.commit().await?;
            Ok(())
        })
        .await
    }

    /// Deletes an action with `action_id` and allows the action to undo the local state. All other
    /// actions that depend on this action are also cancelled.
    ///
    /// To remove an action from the queue without reverting state see [`Queue::delete_action()`].
    ///
    /// To cancel this actions without the dependees see [`Queue::cancel()`].
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed or the action could not be found or another thread
    /// is currently invoking this function.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    pub async fn cancel_with_dependees(&self, action_id: Id) -> QueuedResult<Vec<Id>> {
        let tx = self.stash.connection();
        self.with_exec_guard(async move {
            let mut remaining_actions = vec![action_id];
            tx.transaction().await?;
            let mut sorter = TopologicalSort::<Id>::new();
            let mut cancelled_actions = Vec::new();
            while let Some(action_id) = remaining_actions.pop() {
                let dependees = StoredAction::dependees(&tx, action_id).await.map_err(|e| {
                    error!("Failed to load action dependees: {e}");
                    e
                })?;
                debug!("Dependees: {dependees:?}");
                remaining_actions.extend(dependees.iter().copied());
                for id in dependees {
                    sorter.add_dependency(id, action_id);
                }
            }

            // Cancel all actions in reversed order
            while let Some(action_id) = sorter.pop() {
                let Some(action) = StoredAction::with_id(&tx, action_id).await? else {
                    return Err(QueuedError::ActionNotFound(action_id));
                };

                let (mut decoded, metadata) = self.decode_action(action)?;

                decoded.cancel(&tx, metadata).await?;

                cancelled_actions.push(action_id);
            }
            tx.commit().await?;
            Ok(cancelled_actions)
        })
        .await
    }

    #[tracing::instrument(level = Level::DEBUG, skip(self, session))]
    async fn execute_impl(&self, session: &Session) -> QueuedResult<Option<Id>> {
        let Some(action) = self.next_action().await.map_err(|e| {
            error!("Failed to retrieve action: {e}");
            e
        })?
        else {
            return Ok(None);
        };

        debug!("Next Action: {}", action.short_dbg_str());
        let action_id = action.id;
        let (mut decoded, metadata) = self.decode_action(action)?;

        decoded.execute(self, session, metadata).await?;

        Ok(Some(action_id))
    }

    /// Retrieve the next action to execute.
    pub(crate) async fn next_action(
        &self,
    ) -> std::result::Result<Option<StoredAction>, StashError> {
        let tether = self.stash.connection();
        StoredAction::next(&tether).await
    }

    /// Shared snippet to execute actions locally.
    async fn execute_action_local<T: Action>(
        &self,
        handler: &T::Handler,
        action: &mut T,
        metadata: Metadata,
    ) -> std::result::Result<Id, ActionError<T>> {
        let tx = self.stash.transaction().await?;

        handler.apply_local(action, &tx).await.map_err(|e| {
            error!("Failed to apply local changes: {e}");
            ActionError::Action(e)
        })?;

        let stored_action = StoredAction::new::<T>(action, metadata).map_err(|e| {
            error!("Failed to convert into stored action: {e}");
            Error::from(e)
        })?;

        let id = StoredAction::store(&tx, stored_action).await.map_err(|e| {
            error!("Failed to store action: {e}");
            e
        })?;
        tx.commit().await?;

        Ok(id)
    }

    /// Shared snippet to execute actions remotely.
    async fn execute_action_remote<T: Action>(
        &self,
        id: Id,
        handler: &T::Handler,
        action: &mut T,
        session: &Session,
    ) -> std::result::Result<ActionStatus<T::Output>, ActionError<T>> {
        let tether = self.stash.connection();
        // Note: While we do our bets to check whether this action is still around at the time we
        // are executing this (e.g: concurrent cancel) it is not guaranteed that we will actually
        // be able to observe this reflected in the database at the time of the query.
        self.check_cancelled(&tether, id).await?;

        //1) Attempt to execute on remote
        debug!("Applying action on remote");

        // let post_remote: Result< = post_remote(handler, action, session).await;
        let result = handler.apply_remote(action, session, &self.stash).await;

        match result {
            Ok(result) => {
                // Note: While we do our bets to check whether this action is still around at the time we
                // are executing this (e.g: concurrent cancel) it is not guaranteed that we will actually
                // be able to observe this reflected in the database at the time of the query.
                self.check_cancelled(&tether, id).await?;

                tether.transaction().await?;
                StoredAction::delete(&tether, id).await?;
                tether.commit().await?;

                Ok(ActionStatus::Executed(result))
            }
            Err(e) => {
                error!("Failed to apply on server: {e}");
                if e.is_network_failure() {
                    // if this failed due to network error we should leave it in the queue.
                    return Ok(ActionStatus::Queued(id));
                }

                // Revert local changes and remove action from queue.
                if let Err(e) = async {
                    tether.transaction().await?;
                    handler
                        .revert_local(action, &tether)
                        .await
                        .map_err(ActionError::<T>::Action)?;
                    StoredAction::delete(&tether, id).await?;
                    tether.commit().await?;
                    Ok::<(), ActionError<T>>(())
                }
                .await
                {
                    error!("Failed to revert local changes: {e}");
                }

                Err(ActionError::Action(e))
            }
        }
    }

    /// Check if this action was cancelled/removed.
    async fn check_cancelled(&self, tether: &Tether, id: Id) -> Result<()> {
        // Perform a sanity check before apply local state to make sure that a concurrent
        // request to cancel this action is identified.
        let contains = StoredAction::contains(tether, id).await.map_err(|e| {
            error!("Failed to check if action was cancelled: {e}");
            e
        })?;

        if !contains {
            error!("Action can not continue as it was cancelled");
            return Err(Error::Cancelled(id));
        }
        Ok(())
    }

    fn decode_action(
        &self,
        stored_action: StoredAction,
    ) -> QueuedResult<(Box<dyn QueuedAction>, QueuedMetadata)> {
        let action_id = stored_action.id;
        self.factory.read().decode(stored_action).map_err(|e| {
            error!("Failed to decode action: {e}");
            QueuedError::Factory(action_id, e)
        })
    }

    /// Ensure exclusive access for certain operations on the queue.
    ///
    /// We should only guard the operations below, as executing them in parallel can
    /// potentially lead to state changes being applied multiple times.
    /// * Executing actions
    /// * Cancelling actions
    /// * Deleting actions
    ///
    /// Reading from or queuing action onto the queue can be performed safely from multiple
    /// threads.
    async fn with_exec_guard<T>(
        &self,
        f: impl Future<Output = QueuedResult<T>>,
    ) -> QueuedResult<T> {
        if self
            .exec_guard
            .compare_exchange_weak(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return Err(QueuedError::Busy);
        };

        let r = f.await;

        self.exec_guard.store(false, Ordering::SeqCst);
        r
    }
}

/// Wrapper trait around the actual action type.
pub(crate) trait QueuedAction {
    fn execute<'a, 'q: 'a, 's: 'q>(
        &'a mut self,
        queue: &'q Queue,
        session: &'s Session,
        metadata: QueuedMetadata,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a>>;

    fn cancel<'a>(
        &'a mut self,
        tx: &'a Tether,
        metadata: QueuedMetadata,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a>>;
}

/// Type erasure trait for the action implementation.
pub(crate) struct TypeErasedAction<T: Action> {
    /// Id of the action.
    pub action_id: Id,
    /// Handler of the action.
    pub handler: T::Handler,
    /// The action itself.
    pub action: T,
}

impl<T: Action> QueuedAction for TypeErasedAction<T> {
    fn execute<'a, 'q: 'a, 's: 'q>(
        &'a mut self,
        queue: &'q Queue,
        session: &'s Session,
        metadata: QueuedMetadata,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a>> {
        Box::pin(async {
            // Can't return result here as there is no one to consume it.
            let _ = queue
                .execute_action_remote(self.action_id, &self.handler, &mut self.action, session)
                .await
                .map_err(|e| QueuedError::Action(anyhow::Error::new(e), Box::new(metadata)))?;
            Ok(())
        })
    }

    fn cancel<'a>(
        &'a mut self,
        tx: &'a Tether,
        metadata: QueuedMetadata,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a>> {
        Box::pin(async move {
            // Can't return result here as there is no one to consume it.
            cancel_action_impl(tx, self.action_id, &self.handler, &mut self.action)
                .await
                .map_err(|e| QueuedError::Action(anyhow::Error::new(e), Box::new(metadata)))?;
            Ok(())
        })
    }
}

/// Shared snippet to cancel actions.
async fn cancel_action_impl<T: Action>(
    tx: &Tether,
    id: Id,
    handler: &T::Handler,
    action: &mut T,
) -> std::result::Result<(), ActionError<T>> {
    debug!("Reverting local state");
    // Revert local changes and remove action from queue.
    handler.revert_local(action, tx).await.map_err(|e| {
        error!("Failed to revert local changes: {e}");
        ActionError::Action(e)
    })?;
    StoredAction::delete(tx, id).await.map_err(|e| {
        error!("Failed to delete action: {e}");
        e
    })?;
    Ok(())
}
