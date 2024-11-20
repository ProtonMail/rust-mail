#[cfg(test)]
#[path = "tests/queue.rs"]
mod tests;

use crate::action::{
    Action, Error as ActionErrorTrait, Factory, FactoryError, FactoryResult, Handler, Id, Metadata,
    Priority, Resources, Type,
};
use crate::db::{self, StoredAction};
use chrono::DateTime;
use flume::{Receiver, RecvError, SendError, Sender};
use futures::future::BoxFuture;
use futures::FutureExt;
use parking_lot::RwLock;
use proton_sqlite3::MigratorError;
use stash::orm::Model;
use stash::stash::{Interface, Stash, StashError, Tether};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use topological_sort::TopologicalSort;
use tracing::{debug, error, Level};

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
    Factory(Id, FactoryError),
    #[error("Queued Action error: {0}")]
    Action(anyhow::Error, Box<QueuedMetadata>),
    #[error("DB Error: {0}")]
    DB(#[from] StashError),
    #[error("Action {0} does not exist")]
    ActionNotFound(Id),
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
}

impl Shared {
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
    Queued(Id),
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

/// Output of queueing the [`Action`] with [`Queue::queue_action`] or
/// [`Queue::queue_action_with_metadata`].
///
pub struct QueuedActionOutput<T: Action> {
    /// Result of executing the action locally.
    pub local: T::LocalOutput,
    /// Id of the queued action.
    pub id: Id,
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
        let default_context = Arc::new(());
        let default_context_downgraded = Arc::downgrade(&default_context);
        let shared = Arc::new(Shared {
            stash,
            factory: RwLock::new(factory),
            execution_contexts: RwLock::new(HashMap::new()),
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
        debug!("Queueing action: {} {:?}", T::TYPE, metadata,);
        let handler = T::Handler::default();
        let context = self
            .shared
            .resolve_execution_context::<T>()
            .map_err(|e| ActionError::Queue(e.into()))?;

        let (local_output, id) = execute_action_local(
            &self.shared.stash,
            context.as_ref(),
            &handler,
            &mut action,
            metadata,
        )
        .await?;
        debug!("Action queued with id={id}");

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
        let (sender, receiver) = oneshot::channel();
        debug!("Applying action: {} {:?}", T::TYPE, metadata,);

        let handler = T::Handler::default();

        let context = self
            .shared
            .resolve_execution_context::<T>()
            .map_err(|e| ActionError::Queue(e.into()))?;

        let stash = self.shared.stash.clone();

        let future = async move {
            let output = async {
                // 1) Apply local action and store in the queue
                let (local_output, id) =
                    execute_action_local(&stash, context.as_ref(), &handler, &mut action, metadata)
                        .await?;
                debug!("Action queued with id={id}");

                // 2) Execute remote counter part
                let remote_output =
                    execute_action_remote(&stash, id, context.as_ref(), &handler, &mut action)
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
    pub async fn execute_one(&self) -> QueuedResult<Option<Id>> {
        let (sender, receiver) = oneshot::channel();
        self.sender.send_async(Command::ExecuteOne(sender)).await?;

        receiver.await?
    }

    /// Execute all available actions from the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the queued action could not be executed locally or remotely, or if
    /// another thread is currently invoking this function.
    pub async fn execute_all(&self) -> QueuedResult<()> {
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
    pub async fn delete_action(&self, action_id: Id) -> QueuedResult<()> {
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
    pub async fn contains(&self, action_id: Id) -> Result<bool> {
        let tether = self.shared.stash.connection();
        Ok(StoredAction::contains(&tether, action_id).await?)
    }

    /// Retrieve the metadata associated `action_id` in the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the db query failed.
    pub async fn action(&self, action_id: Id) -> Result<Option<QueuedMetadata>> {
        let tether = self.shared.stash.connection();
        let stored_action = StoredAction::load(action_id, &tether).await?;
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
    pub async fn cancel(&self, action_id: Id) -> QueuedResult<()> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send_async(Command::Cancel(action_id, sender))
            .await?;

        receiver.await?
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
    pub async fn cancel_with_dependees(&self, action_id: Id) -> QueuedResult<Vec<Id>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send_async(Command::CancelDeps(action_id, sender))
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
}

/// Wrapper trait around the actual action type.
pub(crate) trait QueuedAction: Send {
    fn execute<'a, 's: 'a>(
        &'a mut self,
        shared: &Shared,
        metadata: QueuedMetadata,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>>;

    fn cancel<'a>(
        &'a mut self,
        shared: &Shared,
        tx: &'a Tether,
        metadata: QueuedMetadata,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>>;
}

/// Type erasure trait for the action implementation.
pub(crate) struct TypeErasedAction<T: Action + Send> {
    /// Id of the action.
    pub action_id: Id,
    /// Handler of the action.
    pub handler: T::Handler,
    /// The action itself.
    pub action: T,
}

impl<T: Action> QueuedAction for TypeErasedAction<T> {
    fn execute<'a, 's: 'a>(
        &'a mut self,
        shared: &Shared,
        metadata: QueuedMetadata,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>> {
        let result = shared.resolve_execution_context::<T>();
        let stash = shared.stash.clone();
        Box::pin(async move {
            let context = result?;
            // Can't return result here as there is no one to consume it.
            let _ = execute_action_remote(
                &stash,
                self.action_id,
                context.as_ref(),
                &self.handler,
                &mut self.action,
            )
            .await
            .map_err(|e| QueuedError::Action(anyhow::Error::new(e), Box::new(metadata)))?;
            Ok(())
        })
    }

    fn cancel<'a>(
        &'a mut self,
        shared: &Shared,
        tx: &'a Tether,
        metadata: QueuedMetadata,
    ) -> Pin<Box<dyn Future<Output = QueuedResult<()>> + 'a + Send>> {
        let result = shared.resolve_execution_context::<T>();
        Box::pin(async move {
            let context = result?;
            // Can't return result here as there is no one to consume it.
            cancel_action_impl(
                tx,
                self.action_id,
                context.as_ref(),
                &self.handler,
                &mut self.action,
            )
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
    context: &T::Context,
    handler: &T::Handler,
    action: &mut T,
) -> std::result::Result<(), ActionError<T>> {
    debug!("Reverting local state");
    // Revert local changes and remove action from queue.
    handler
        .revert_local(context, action, tx)
        .await
        .map_err(|e| {
            error!("Failed to revert local changes: {e}");
            ActionError::Action(e)
        })?;
    StoredAction::delete(tx, id).await.map_err(|e| {
        error!("Failed to delete action: {e}");
        e
    })?;
    Ok(())
}

/// Worker commands
enum Command {
    /// Run immediate action
    Apply(BoxFuture<'static, ()>),
    /// Execute one queued action
    ExecuteOne(oneshot::Sender<QueuedResult<Option<Id>>>),
    /// Execute all queued actions
    ExecuteAll(oneshot::Sender<QueuedResult<()>>),
    /// Cancel an action
    Cancel(Id, oneshot::Sender<QueuedResult<()>>),
    /// Cancel an action and all the actions which depend on this action
    CancelDeps(Id, oneshot::Sender<QueuedResult<Vec<Id>>>),
    /// Delete an action without cancelling
    Delete(Id, oneshot::Sender<QueuedResult<()>>),
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
                    let r = self.execute_impl().await;
                    if tx.send(r).is_err() {
                        error!("Failed to send execute one result back to callee");
                    }
                }
                Command::Cancel(id, tx) => {
                    self.wait_on_tasks().await;
                    let r = self.cancel(id).await;
                    if tx.send(r).is_err() {
                        error!("Failed to send cancel result back to callee");
                    }
                }
                Command::CancelDeps(id, tx) => {
                    self.wait_on_tasks().await;
                    let r = self.cancel_with_dependees(id).await;
                    if tx.send(r).is_err() {
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
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    async fn execute_impl(&self) -> QueuedResult<Option<Id>> {
        let Some(action) = self.next_action().await.map_err(|e| {
            error!("Failed to retrieve action: {e}");
            e
        })?
        else {
            return Ok(None);
        };

        let action_id = action.id.unwrap();
        debug!(
            "Next Action: id={} type={} debug={}",
            action_id,
            action.action_type,
            action.short_dbg_str()
        );
        let (mut decoded, metadata) = self.decode_action(action)?;

        decoded.execute(&self.shared, metadata).await?;

        Ok(Some(action_id))
    }

    /// See [`Queue::execute_all()`] for more details.

    async fn execute_all(&self) -> QueuedResult<()> {
        while self.execute_impl().await?.is_some() {}
        Ok(())
    }

    async fn next_action(&self) -> std::result::Result<Option<StoredAction>, StashError> {
        let tether = self.shared.stash.connection();
        StoredAction::next(&tether).await
    }
    /// See [`Queue::delete()`] for more details.
    async fn delete(&self, action_id: Id) -> QueuedResult<()> {
        let tx = self.shared.stash.transaction().await?;
        StoredAction::delete(&tx, action_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// See [`Queue::cancel()`] for more details.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    async fn cancel(&self, action_id: Id) -> QueuedResult<()> {
        let conn = self.shared.stash.connection();
        let Some(action) = StoredAction::load(action_id, &conn).await? else {
            return Err(QueuedError::ActionNotFound(action_id));
        };

        let (mut decoded, metadata) = self.decode_action(action)?;
        conn.transaction().await?;
        decoded.cancel(&self.shared, &conn, metadata).await?;
        conn.commit().await?;
        Ok(())
    }

    /// See [`Queue::cancel_with_dependees()`] for more details.
    #[tracing::instrument(level = Level::DEBUG, skip(self))]
    async fn cancel_with_dependees(&self, action_id: Id) -> QueuedResult<Vec<Id>> {
        let tx = self.shared.stash.transaction().await?;
        let mut remaining_actions = vec![action_id];
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
            let Some(action) = StoredAction::load(action_id, &tx).await? else {
                return Err(QueuedError::ActionNotFound(action_id));
            };

            let (mut decoded, metadata) = self.decode_action(action)?;

            decoded.cancel(&self.shared, &tx, metadata).await?;

            cancelled_actions.push(action_id);
        }
        tx.commit().await?;
        Ok(cancelled_actions)
    }

    /// Wait on all the executing immediate actions.
    async fn wait_on_tasks(&mut self) {
        while self.apply_tasks.join_next().await.is_some() {}
    }

    /// Decode stored action and return an executor.
    fn decode_action(
        &self,
        stored_action: StoredAction,
    ) -> QueuedResult<(Box<dyn QueuedAction>, QueuedMetadata)> {
        let action_id = stored_action.id.unwrap();
        self.shared
            .factory
            .read()
            .decode(stored_action)
            .map_err(|e| {
                error!("Failed to decode action: {e}");
                QueuedError::Factory(action_id, e)
            })
    }
}

/// Shared snippet to execute actions locally.
async fn execute_action_local<T: Action>(
    stash: &Stash,
    context: &T::Context,
    handler: &T::Handler,
    action: &mut T,
    metadata: Metadata,
) -> std::result::Result<(T::LocalOutput, Id), ActionError<T>> {
    let tx = stash.transaction().await?;

    let local_output = handler
        .apply_local(context, action, &tx)
        .await
        .map_err(|e| {
            error!("Failed to apply local changes: {e}");
            ActionError::Action(e)
        })?;

    let mut stored_action = StoredAction::new::<T>(action, metadata).map_err(|e| {
        error!("Failed to convert into stored action: {e}");
        Error::from(e)
    })?;

    stored_action.save_using(&tx).await.map_err(|e| {
        error!("Failed to store action: {e}");
        e
    })?;
    tx.commit().await?;

    Ok((local_output, stored_action.id.unwrap()))
}

/// Shared snippet to execute actions remotely.
async fn execute_action_remote<T: Action>(
    stash: &Stash,
    id: Id,
    context: &T::Context,
    handler: &T::Handler,
    action: &mut T,
) -> std::result::Result<ActionRemoteOutput<T::RemoteOutput>, ActionError<T>> {
    let tether = stash.connection();

    //1) Attempt to execute on remote
    debug!("Applying action on remote");

    // let post_remote: Result< = post_remote(handler, action, session).await;
    let result = handler.apply_remote(context, action, stash).await;

    match result {
        Ok(result) => {
            tether.transaction().await?;
            StoredAction::delete(&tether, id).await?;
            tether.commit().await?;

            Ok(ActionRemoteOutput::Executed(result))
        }
        Err(e) => {
            error!("Failed to apply on server: {e}");
            if e.is_network_failure() {
                // if this failed due to network error we should leave it in the queue.
                return Ok(ActionRemoteOutput::Queued(id));
            }

            // Revert local changes and remove action from queue.
            if let Err(e) = async {
                tether.transaction().await?;
                handler
                    .revert_local(context, action, &tether)
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
