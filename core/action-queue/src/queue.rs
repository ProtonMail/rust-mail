#[cfg(test)]
#[path = "tests/queue.rs"]
mod tests;

use crate::action::{
    Action, ActionId, Error as ActionErrorTrait, Factory, FactoryError, FactoryResult, Handler,
    Metadata, Priority, Resources, Type, WriterGuard,
};
use crate::db::{self, ExecutionGuard, ExecutionGuardError, StoredAction};
use chrono::DateTime;
use flume::{Receiver, RecvError, SendError, Sender};
use futures::future::BoxFuture;
use futures::FutureExt;
use parking_lot::RwLock;
use proton_sqlite3::MigratorError;
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashError, Tether};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use topological_sort::TopologicalSort;
use tracing::{debug, debug_span, error, Instrument, Level};
use uuid::Uuid;

/// Execution context errors
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("Could not find execution context for {0}")]
    ContextNotFound(Type),
    #[error("Could not upgrade execution context for {0}")]
    ContextUpgrade(Type),
}

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
    #[error("{0}")]
    Context(#[from] ContextError),
    #[error("Failed to communicate with worker")]
    WorkerChannel,
    #[error("Unknown action: {0}")]
    UnknownAction(String),
    #[error("Replacing an action with dependencies to self")]
    SelfReferenceDependency,
}

impl<T> From<SendError<T>> for Error {
    fn from(_: SendError<T>) -> Self {
        Self::WorkerChannel
    }
}

impl From<RecvError> for Error {
    fn from(_: RecvError) -> Self {
        Self::WorkerChannel
    }
}

impl From<oneshot::error::RecvError> for Error {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self::WorkerChannel
    }
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
    Factory(ActionId, FactoryError),
    #[error("Queued Action error: {0}")]
    Action(Arc<anyhow::Error>, Arc<QueuedMetadata>),
    #[error("DB Error: {0}")]
    DB(#[from] StashError),
    #[error("Action {0} does not exist")]
    ActionNotFound(ActionId),
    #[error("{0}")]
    Context(#[from] ContextError),
    #[error("Failed to communicate with worker")]
    WorkerChannel,
}

impl<T> From<SendError<T>> for QueuedError {
    fn from(_: SendError<T>) -> Self {
        Self::WorkerChannel
    }
}

impl From<RecvError> for QueuedError {
    fn from(_: RecvError) -> Self {
        Self::WorkerChannel
    }
}

impl From<oneshot::error::RecvError> for QueuedError {
    fn from(_: oneshot::error::RecvError) -> Self {
        Self::WorkerChannel
    }
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
    pub dependencies: Vec<ActionId>,
    /// Optional debug string associated with this action.
    pub debug_string: Option<String>,
    /// Resources which were associated with this action.
    pub resources: Resources,
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
        }
    }
}

/// Broadcast message issued when actions are executed in the background so their
/// progress can be tracked and potentially awaited on.
#[derive(Debug, Clone)]
pub enum BroadcastMessage {
    /// This queued action was executed successfully
    Success(ActionId),
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
/// There are two modes of execution immediate and queued.
///
/// # Immediate Mode
///
/// As the name indicated, immediate mode attempts to execute the actions immediately. However,
/// if we detect that we can't apply the action on the remote due to lack of network, the action
/// is automatically queued.
///
/// If the action can complete all steps successfully [`Action::RemoteOutput`] is returned as part of
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
///
/// # Execution Contexts
///
/// Every action can be assigned an execution context which contains runtime
/// data which is not represented by this API. The execution contexts need
/// to be registered with the queue upfront so that they can also
/// be used by actions that are queued for later execution.
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
/// isolation. If more than one location attempts to call these functions currently
/// we will return [`QueuedError::Busy`].
///
pub struct Queue {
    shared: Arc<Shared>,
    // Keep the default context alive so that it is available for any action
    // which does not need a custom context.
    _default_context: Arc<()>,
    sender: Sender<Command>,
}

/// Internal shared data used by the [`Queue`] and [`BackgroundWorker`].
pub(crate) struct Shared {
    stash: Stash,
    factory: RwLock<Factory>,
    execution_contexts: RwLock<HashMap<TypeId, Weak<dyn Any + Send + Sync>>>,
    broadcast_sender: tokio::sync::broadcast::Sender<BroadcastMessage>,
}

impl Shared {
    fn has_action<T: Action>(&self) -> bool {
        self.factory.read().has_action::<T>()
    }

    fn resolve_execution_context<T: Action>(
        &self,
    ) -> std::result::Result<Arc<T::Context>, ContextError> {
        let type_id = TypeId::of::<T::Context>();
        let exec_contexts = self.execution_contexts.read();
        let context = exec_contexts
            .get(&type_id)
            .ok_or(ContextError::ContextNotFound(T::TYPE))?;
        let context = context
            .upgrade()
            .ok_or(ContextError::ContextUpgrade(T::TYPE))?;
        Ok(context.downcast::<T::Context>().expect("Should not fail"))
    }
}

/// Output of the [`Action`] after being applied with [`Queue::apply_action`] or
/// [`Queue::apply_action_with_metadata`].
pub enum ActionRemoteOutput<Remote> {
    /// Action was executed successfully on local and on remote.
    Executed(Remote),
    /// Action could not be executed on the remote at this time and was queued.
    Queued(ActionId),
}

/// Output of applying the [`Action`] with [`Queue::apply_action`] or
/// [`Queue::apply_action_with_metadata`].
///
/// If remote result depends on whether the action was queued or
/// executed immediately.
pub struct ActionOutput<T: Action> {
    /// Result of executing the action locally.
    pub local: T::LocalOutput,
    /// Result of executing the action on the remote server.
    pub remote: ActionRemoteOutput<T::RemoteOutput>,
}

impl<T> Default for ActionOutput<T>
where
    T: Action,
    T::LocalOutput: Default,
    T::RemoteOutput: Default,
{
    fn default() -> Self {
        Self {
            local: T::LocalOutput::default(),
            remote: ActionRemoteOutput::Executed(T::RemoteOutput::default()),
        }
    }
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
        let mut tether = stash.connection();
        db::create_tables(&mut tether).await?;
        let default_context = Arc::new(());
        let default_context_downgraded = Arc::downgrade(&default_context);
        let (sender, _) = tokio::sync::broadcast::channel(32);
        let shared = Arc::new(Shared {
            stash,
            factory: RwLock::new(factory),
            execution_contexts: RwLock::new(HashMap::new()),
            broadcast_sender: sender,
        });
        let (sender, receiver) = flume::bounded::<Command>(16);
        let mut worker = BackgroundWorker::new(receiver, Arc::clone(&shared.clone()));
        tokio::spawn(async move {
            worker.run().await;
        });
        let queue = Self {
            shared,
            _default_context: default_context,
            sender,
        };

        queue.register_execution_context(default_context_downgraded);
        Ok(queue)
    }

    /// Register an [`Action`] with the factory.
    ///
    /// # Errors
    ///
    /// Returns error if the action type was already registered before.
    pub fn register<T: Action>(&self) -> FactoryResult<()> {
        self.shared.factory.write().register::<T>()
    }

    /// Register an execution context with the queue.
    ///
    /// Execution context are used by actions to access runtime data.
    ///
    pub fn register_execution_context<E: Any + Send + Sync + 'static>(&self, context: Weak<E>) {
        self.shared
            .execution_contexts
            .write()
            .insert(TypeId::of::<E>(), context);
    }

    /// Return the database associated with the queue.
    #[must_use]
    pub fn stash(&self) -> &Stash {
        &self.shared.stash
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
    ) -> std::result::Result<QueuedActionOutput<T>, ActionError<T>> {
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
    ) -> std::result::Result<QueuedActionOutput<T>, ActionError<T>> {
        debug!("Queueing action: {} {:?}", T::TYPE, metadata);
        if !self.shared.has_action::<T>() {
            error!("Unknown action queued: {}", T::TYPE);
            return Err(Error::UnknownAction(T::TYPE.to_string()).into());
        }
        let handler = T::Handler::default();
        let context = self
            .shared
            .resolve_execution_context::<T>()
            .map_err(|e| ActionError::Queue(e.into()))?;

        let (local_output, id) = execute_action_local(
            &self.shared,
            context.as_ref(),
            &handler,
            &mut action,
            metadata,
            None,
        )
        .await?;
        debug!("Action queued with id={id}");

        Ok(QueuedActionOutput {
            local: local_output,
            id,
        })
    }

    /// Attempt to replace an existing action with an updated version. If the action no longer
    /// exists or the types do not match, a new action will be queued instead.
    ///
    /// A default [`Metadata`] type is assigned to this `action`.
    ///
    /// # Errors
    ///
    /// Returns error if action could not be executed locally.
    pub async fn replace_or_queue_action<T: Action>(
        &self,
        existing_id: ActionId,
        action: T,
    ) -> std::result::Result<QueuedActionOutput<T>, ActionError<T>> {
        self.replace_or_queue_action_with_metadata::<T>(existing_id, action, Metadata::default())
            .await
    }

    /// Attempt to replace an existing action with an updated version. If the action no longer
    /// exists or the types do not match, a new action will be queued instead.
    ///
    /// # Errors
    ///
    /// Returns error if action could not be executed locally.
    #[tracing::instrument(level = Level::DEBUG, skip(self, metadata, action), name =
    "QueueAction")]
    pub async fn replace_or_queue_action_with_metadata<T: Action>(
        &self,
        existing_id: ActionId,
        mut action: T,
        metadata: Metadata,
    ) -> std::result::Result<QueuedActionOutput<T>, ActionError<T>> {
        debug!(
            "Replacing {existing_id:?} or Queueing action: {} {metadata:?}",
            T::TYPE,
        );
        if metadata.dependencies.contains(&existing_id) {
            return Err(Error::SelfReferenceDependency.into());
        }

        if !self.shared.has_action::<T>() {
            error!("Unknown action queued: {}", T::TYPE);
            return Err(Error::UnknownAction(T::TYPE.to_string()).into());
        }

        let handler = T::Handler::default();
        let context = self
            .shared
            .resolve_execution_context::<T>()
            .map_err(|e| ActionError::Queue(e.into()))?;

        let shared = Arc::clone(&self.shared);

        let (local_output, id) = execute_action_local(
            &shared,
            context.as_ref(),
            &handler,
            &mut action,
            metadata,
            Some(existing_id),
        )
        .await?;
        if existing_id == id {
            debug!("Action has been updated");
        } else {
            debug!("Action queued with id={id}");
        }

        Ok(QueuedActionOutput {
            local: local_output,
            id,
        })
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
        action: T,
    ) -> std::result::Result<ActionOutput<T>, ActionError<T>> {
        self.apply_action_with_metadata::<T>(action, Metadata::default())
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
    #[tracing::instrument(level = Level::DEBUG, skip(self, metadata, action), name =
    "ApplyAction")]
    pub async fn apply_action_with_metadata<T: Action>(
        &self,
        mut action: T,
        metadata: Metadata,
    ) -> std::result::Result<ActionOutput<T>, ActionError<T>> {
        debug!("Applying action: {} {metadata:?}", T::TYPE);
        if !self.shared.has_action::<T>() {
            error!("Unknown action applied: {}", T::TYPE);
            return Err(Error::UnknownAction(T::TYPE.to_string()).into());
        }

        let (sender, receiver) = oneshot::channel();

        let handler = T::Handler::default();

        let context = self
            .shared
            .resolve_execution_context::<T>()
            .map_err(|e| ActionError::Queue(e.into()))?;

        let shared = Arc::clone(&self.shared);

        let future = async move {
            let output = async {
                // 1) Apply local action and store in the queue
                let (local_output, id) = execute_action_local(
                    &shared,
                    context.as_ref(),
                    &handler,
                    &mut action,
                    metadata,
                    None,
                )
                .await?;
                debug!("Action queued with id={id}");

                let mut tether = shared.stash.connection();
                let tx = tether.transaction().await?;
                let executor_id = Uuid::new_v4().to_string();
                let guard = ExecutionGuard::acquire(id, executor_id, &tx).await?;
                tx.commit().await?;

                // 2) Execute remote counter part
                let remote_output = execute_action_remote(
                    &shared,
                    id,
                    context.as_ref(),
                    &handler,
                    &mut action,
                    &mut tether,
                    guard,
                )
                .await?;

                Ok(ActionOutput {
                    local: local_output,
                    remote: remote_output,
                })
            }
            .await;
            let _ = sender.send(output).inspect_err(|_| {
                error!("Failed to send result from apply action back to callee");
            });
        }
        .boxed();

        // Unlike Queued actions which are only execute by the background worker,
        // immediate actions can cause conflict with the background worker as
        // they can be picked up as the next action in the list.
        // To prevent concurrent conflicts, we execute this action on the worker
        // and wait for the result.
        self.sender
            .send_async(Command::Apply(future))
            .await
            .map_err(|_| Error::WorkerChannel)?;

        receiver
            .await
            .map_err(|_| ActionError::Queue(Error::WorkerChannel))?
    }

    /// Execute one action from the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the queued action could not be executed locally or remotely, or if
    /// another thread is currently invoking this function.
    pub async fn execute_one(&self) -> QueuedResult<Option<ActionId>> {
        let (sender, receiver) = oneshot::channel();
        self.sender.send_async(Command::ExecuteOne(sender)).await?;

        receiver.await?
    }

    /// Execute all available actions from the queue.
    ///
    /// Returns the number of executed actions.
    ///
    /// # Errors
    ///
    /// Returns error if the queued action could not be executed locally or remotely, or if
    /// another thread is currently invoking this function.
    pub async fn execute_all(&self) -> QueuedResult<usize> {
        let (sender, receiver) = oneshot::channel();
        self.sender.send_async(Command::ExecuteAll(sender)).await?;

        receiver.await?
    }

    /// Delete an action with `action_id` from the queue *without reverting local state*.
    ///
    /// To revert local state use [`Queue::cancel()`] or [`Queue::cancel_with_dependees()`].
    ///
    /// # Errors
    ///
    /// Returns error if the db operation failed or if another thread is currently invoking
    /// this function.
    pub async fn delete_action(&self, action_id: ActionId) -> QueuedResult<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send_async(Command::Delete(action_id, sender))
            .await?;

        receiver.await?
    }

    /// Returns the number of actions queued.
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed.
    pub async fn queued_actions_count(&self) -> Result<u64> {
        let tether = self.shared.stash.connection();
        Ok(StoredAction::pending_count(&tether).await?)
    }

    /// Check whether the action with `action_id` is present in the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed.
    pub async fn contains(&self, action_id: ActionId) -> Result<bool> {
        let tether = self.shared.stash.connection();
        Ok(StoredAction::contains(&tether, action_id).await?)
    }

    /// Retrieve the metadata associated `action_id` in the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed.
    pub async fn action(&self, action_id: ActionId) -> Result<Option<QueuedMetadata>> {
        let tether = self.shared.stash.connection();
        let stored_action = StoredAction::load(action_id, &tether).await?;
        Ok(stored_action.map(QueuedMetadata::from))
    }

    /// Deletes an action with `action_id` and allows the action to undo the local state. All other
    /// actions that depend on this action are also cancelled.
    ///
    /// To remove an action from the queue without reverting state see [`Queue::delete_action()`].
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed or the action could not be found or another thread
    /// is currently invoking this function.
    pub async fn cancel(&self, action_id: ActionId) -> QueuedResult<Vec<ActionId>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send_async(Command::Cancel(action_id, sender))
            .await?;

        receiver.await?
    }

    /// Retrieve the next action to execute.
    #[cfg(test)]
    pub(crate) async fn next_action(
        &self,
    ) -> std::result::Result<Option<StoredAction>, StashError> {
        let tether = self.shared.stash.connection();
        StoredAction::next(&tether).await
    }

    /// Create a new broadcast receiver to observe the state of queued actions.
    #[must_use]
    pub fn new_broadcast_receiver(&self) -> tokio::sync::broadcast::Receiver<BroadcastMessage> {
        self.shared.broadcast_sender.subscribe()
    }
}

/// Indicates the state of the action.
pub enum QueuedActionState {
    /// The action was executed, which led to either a success or failure result.
    Executed(ActionId),
    /// The action was deferred due to lack of network.
    Queued(ActionId),
}

/// Wrapper trait around the actual action type.
pub(crate) trait QueuedAction: Send {
    fn execute<'a, 's: 'a>(
        &'a mut self,
        shared: &'a Shared,
        tether: &'a mut Tether,
        execution_guard: ExecutionGuard,
        metadata: Arc<QueuedMetadata>,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<QueuedActionState>> + 'a + Send>>;

    fn cancel<'a>(
        &'a mut self,
        shared: &'a Shared,
        tx: &'a Bond,
        metadata: Arc<QueuedMetadata>,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>>;
}

/// Type erasure trait for the action implementation.
pub(crate) struct TypeErasedAction<T: Action + Send> {
    /// Id of the action.
    pub action_id: ActionId,
    /// Handler of the action.
    pub handler: T::Handler,
    /// The action itself.
    pub action: T,
}

impl<T: Action> QueuedAction for TypeErasedAction<T> {
    fn execute<'a, 's: 'a>(
        &'a mut self,
        shared: &'a Shared,
        tether: &'a mut Tether,
        exec_guard: ExecutionGuard,
        metadata: Arc<QueuedMetadata>,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<QueuedActionState>> + 'a + Send>> {
        let result = shared.resolve_execution_context::<T>();
        Box::pin(async move {
            let context = result?;
            // Can't return result here as there is no one to consume it.
            let output = execute_action_remote(
                shared,
                self.action_id,
                context.as_ref(),
                &self.handler,
                &mut self.action,
                tether,
                exec_guard,
            )
            .await
            .map_err(|e| QueuedError::Action(Arc::new(anyhow::Error::new(e)), metadata))?;

            Ok(match output {
                ActionRemoteOutput::Executed(_) => QueuedActionState::Executed(self.action_id),
                ActionRemoteOutput::Queued(id) => QueuedActionState::Queued(id),
            })
        })
    }

    fn cancel<'a>(
        &'a mut self,
        shared: &'a Shared,
        tx: &'a Bond,
        metadata: Arc<QueuedMetadata>,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>> {
        let result = shared.resolve_execution_context::<T>();
        Box::pin(async move {
            let context = result?;
            // Can't return result here as there is no one to consume it.
            debug!(
                "Reverting local state for {} type={}",
                self.action_id,
                T::TYPE
            );
            // Revert local changes and remove action from queue.
            if let Err(e) = self
                .handler
                .revert_local(self.action_id, &context, &mut self.action, tx)
                .await
            {
                error!("Failed to revert local changes: {e:?}");
            }
            StoredAction::delete(tx, self.action_id)
                .await
                .map_err(|e| {
                    error!("Failed to delete action: {e:?}");
                    e
                })
                .map_err(|e| QueuedError::Action(Arc::new(anyhow::Error::new(e)), metadata))?;
            Ok(())
        })
    }
}

/// Worker commands
enum Command {
    /// Run immediate action
    Apply(BoxFuture<'static, ()>),
    /// Execute one queued action
    ExecuteOne(oneshot::Sender<QueuedResult<Option<ActionId>>>),
    /// Execute all queued actions
    ExecuteAll(oneshot::Sender<QueuedResult<usize>>),
    /// Cancel an action and all the actions which depend on this action
    Cancel(ActionId, oneshot::Sender<QueuedResult<Vec<ActionId>>>),
    /// Delete an action without cancelling
    Delete(ActionId, oneshot::Sender<QueuedResult<()>>),
}

impl Debug for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Apply(_) => {
                write!(f, "Command::Apply")
            }
            Command::ExecuteOne(_) => {
                write!(f, "Command::ExecuteOne")
            }
            Command::ExecuteAll(_) => {
                write!(f, "Command::ExecuteAll")
            }
            Command::Cancel(id, _) => {
                write!(f, "Command::Cancel({id}")
            }
            Command::Delete(id, _) => {
                write!(f, "Command::Delete({id})")
            }
        }
    }
}

/// The background worker enforces a single execution scope.
///
/// While it is safe to queue action in parallel with queued action execution,
/// the same does not apply for immediate actions. It was possible
/// in the previous iteration for the background executor to attempt to
/// execute an immediate action at the same time leading to odd action
/// cancelled errors.
///
/// The background worker now ensures that only one location is actively
/// executing queued actions at any given time and immediate actions are
/// also send to the worker. Immediate actions can still run in parallel,
/// but we don't allow other queued commands to proceed until the current
/// set of running immediate actions has finished executing.
///
struct BackgroundWorker {
    receiver: Receiver<Command>,
    shared: Arc<Shared>,
    /// St of running immediate actions
    apply_tasks: JoinSet<()>,
}

impl BackgroundWorker {
    fn new(receiver: Receiver<Command>, shared: Arc<Shared>) -> Self {
        Self {
            receiver,
            shared,
            apply_tasks: JoinSet::default(),
        }
    }
    async fn run(&mut self) {
        debug!("Starting action queue background worker");
        // To allow multiple actions to be applied immediately to the queue in
        // parallel, we spawn a dedicated task for each we receive.
        // Other operations on the queue need to wait until the current task
        // set complete to prevent picking up actions that are already
        // running.
        while let Ok(cmd) = self.receiver.recv_async().await {
            debug!("Received command: {cmd:?}");
            match cmd {
                // return channel is embedded in the future due to type erasure.
                Command::Apply(future) => {
                    self.apply_tasks.spawn(future);
                }
                Command::ExecuteAll(tx) => {
                    self.wait_on_tasks().await;
                    let r = self.execute_all().await;
                    if tx.send(r).is_err() {
                        error!("Failed to send execute one result back to callee");
                    }
                }
                Command::ExecuteOne(tx) => {
                    self.wait_on_tasks().await;
                    let mut tether = self.shared.stash.connection();
                    let r = self.execute_impl(&mut tether).await;

                    if tx
                        .send(r.map(|v| {
                            if let Some(QueuedActionState::Executed(value)) = v {
                                Some(value)
                            } else {
                                None
                            }
                        }))
                        .is_err()
                    {
                        error!("Failed to send execute one result back to callee");
                    }
                }
                Command::Cancel(id, tx) => {
                    self.wait_on_tasks().await;
                    let r = self.cancel(id).await;
                    if tx
                        .send(r.map(|v| v.into_iter().map(|v| v.id).collect()))
                        .is_err()
                    {
                        error!("Failed to send cancel result back to callee");
                    }
                }
                Command::Delete(id, tx) => {
                    self.wait_on_tasks().await;
                    let r = self.delete(id).await;
                    if tx.send(r).is_err() {
                        error!("Failed to send delete result back to callee");
                    }
                }
            }
        }
        debug!("Terminating action queue background worker");
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
        debug!(
            "Next Action: id={} type={} debug={}",
            action_id,
            action_type,
            action.short_dbg_str()
        );

        async {
            let (mut decoded, metadata) = decode_action(&self.shared.factory, action)?;

            let exec_output = decoded
                .execute(&self.shared, tether, exec_guard, metadata)
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
                .send(BroadcastMessage::Success(action_id));

            Ok(Some(exec_output))
        }
        .instrument(debug_span!("QueuedExecute", id=?action_id, type=?action_type))
        .await
    }

    /// See [`Queue::execute_all()`] for more details.
    async fn execute_all(&self) -> QueuedResult<usize> {
        let mut tether = self.shared.stash.connection();
        let mut counter = 0;
        while let Some(QueuedActionState::Executed(_)) = self.execute_impl(&mut tether).await? {
            counter += 1;
        }
        Ok(counter)
    }

    async fn next_action(
        &self,
        tether: &mut Tether,
    ) -> std::result::Result<Option<(ExecutionGuard, StoredAction)>, StashError> {
        StoredAction::pop("BackgroundExecutor".to_owned(), tether).await
    }
    /// See [`Queue::delete()`] for more details.
    async fn delete(&mut self, action_id: ActionId) -> QueuedResult<()> {
        let mut tether = self.shared.stash.connection();
        let tx = tether.transaction().await?;
        let existing_action_type = StoredAction::delete(&tx, action_id).await?;
        tx.commit().await?;
        if let Some(existing_action_type) = existing_action_type {
            // Send only fails if there are no receivers, which is a valid state.
            let _ = self.shared.broadcast_sender.send(BroadcastMessage::Deleted(
                action_id,
                Arc::new(existing_action_type),
            ));
        }
        Ok(())
    }

    /// See [`Queue::cancel()`] for more details.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    async fn cancel(&self, action_id: ActionId) -> QueuedResult<Vec<Arc<QueuedMetadata>>> {
        let mut tether = self.shared.stash.connection();
        let tx = tether.transaction().await?;
        let cancelled_actions = cancel_action_with_dependees(&self.shared, &tx, action_id).await?;
        tx.commit().await?;
        for cancelled_action in &cancelled_actions {
            // Send only fails if there are no receivers, which is a valid state.
            let _ = self
                .shared
                .broadcast_sender
                .send(BroadcastMessage::Cancelled(Arc::clone(cancelled_action)));
        }
        Ok(cancelled_actions)
    }

    /// Wait on all the executing immediate actions.
    async fn wait_on_tasks(&mut self) {
        while self.apply_tasks.join_next().await.is_some() {}
    }
}

/// Shared snippet to execute actions locally.
async fn execute_action_local<T: Action>(
    shared: &Shared,
    context: &T::Context,
    handler: &T::Handler,
    action: &mut T,
    metadata: Metadata,
    existing_id: Option<ActionId>,
) -> std::result::Result<(T::LocalOutput, ActionId), ActionError<T>> {
    let mut tether = shared.stash.connection();
    let tx = tether.transaction().await?;

    // Create the action record.
    let mut stored_action = StoredAction::without_state::<T>(metadata);
    if let Some(exising_id) = existing_id {
        stored_action
            .create_or_update(exising_id, &tx)
            .await
            .map_err(|e| {
                error!("Failed to create or update action: {e:?}");
                e
            })?;
    } else {
        stored_action.save(&tx).await.map_err(|e| {
            error!("Failed to store action: {e:?}");
            e
        })?;
    }

    // Execute the local changes
    let local_output = handler
        .apply_local(stored_action.id.unwrap(), context, action, &tx)
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
        .update_action_state(&tx)
        .await
        .inspect_err(|e| {
            error!("Failed to update action state: {e:?}");
        })?;

    tx.commit().await?;

    Ok((local_output, stored_action.id.unwrap()))
}
/// Shared snippet to execute actions remotely.
async fn execute_action_remote<T: Action>(
    shared: &Shared,
    id: ActionId,
    context: &T::Context,
    handler: &T::Handler,
    action: &mut T,
    tether: &mut Tether,
    guard: ExecutionGuard,
) -> std::result::Result<ActionRemoteOutput<T::RemoteOutput>, ActionError<T>> {
    //1) Attempt to execute on remote
    debug!("Applying action on remote");

    let writer_guard = WriterGuard::new(tether, &guard);
    let result = handler
        .apply_remote(id, context, action, writer_guard)
        .await;
    let mut cancelled_actions = vec![];
    let bond = match guard.transaction(tether).await {
        Ok(tx) => tx,
        Err(ExecutionGuardError::Expired) => {
            return Ok(ActionRemoteOutput::Queued(id));
        }
        Err(ExecutionGuardError::Stash(e)) => return Err(e.into()),
    };
    let result = async {
        match result {
            Ok(result) => {
                StoredAction::delete(&bond, id).await?;

                debug!("Action executed");
                Ok(ActionRemoteOutput::Executed(result))
            }
            Err(e) => {
                error!("Failed to apply on server: {e:?}");
                if e.is_network_failure() {
                    debug!("Action remains in queue due to lack of network");
                    // if this failed due to network error we should leave it in the queue.
                    return Ok(ActionRemoteOutput::Queued(id));
                } else if e.is_writer_guard_expired() {
                    debug!("Action remains in queue due to expired writer guard");
                    return Ok(ActionRemoteOutput::Queued(id));
                }
                debug!("Reverting self and dependees");
                match cancel_action_with_dependees(shared, &bond, id).await {
                    Ok(ids) => {
                        cancelled_actions = ids;
                    }
                    Err(e) => {
                        error!("Failed to cancel action and depeendees: {e:?}");
                    }
                }

                Err(ActionError::Action(e))
            }
        }
    }
    .await;

    guard.release(&bond).await?;
    bond.commit().await?;
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

/// Cancel
async fn cancel_action_with_dependees(
    shared: &Shared,
    bond: &Bond<'_>,
    action_id: ActionId,
) -> QueuedResult<Vec<Arc<QueuedMetadata>>> {
    let mut remaining_actions = vec![action_id];
    let mut sorter = TopologicalSort::<ActionId>::new();
    let mut cancelled_actions = Vec::new();
    while let Some(action_id) = remaining_actions.pop() {
        let dependees = StoredAction::dependees(bond, action_id)
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

        decoded.cancel(shared, bond, Arc::clone(&metadata)).await?;

        cancelled_actions.push(metadata);
    } else {
        debug!("Reverting {} dependent actions", sorter.len());
        // Cancel all actions in reversed order
        while let Some(current_action_id) = sorter.pop() {
            let Some(action) = StoredAction::load(current_action_id, bond).await? else {
                return Err(QueuedError::ActionNotFound(current_action_id));
            };

            let (mut decoded, metadata) = decode_action(&shared.factory, action)?;

            decoded.cancel(shared, bond, Arc::clone(&metadata)).await?;

            cancelled_actions.push(metadata);
        }
    }
    Ok(cancelled_actions)
}

/// Decode stored action and return an executor.
fn decode_action(
    factory: &RwLock<Factory>,
    stored_action: StoredAction,
) -> QueuedResult<(Box<dyn QueuedAction>, Arc<QueuedMetadata>)> {
    let action_id = stored_action.id.unwrap();
    factory.read().decode(stored_action).map_err(|e| {
        error!("Failed to decode action: {e:?}");
        QueuedError::Factory(action_id, e)
    })
}
