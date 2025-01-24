//! This structure is used to prevent `event_loop` and `action_queue` to run at the same time.
//!
//! The `event_loop` can potentially update any values and an action running at the same time
//! could use data that changed between there acquisition and there use leading into data
//! corruption.
//!
//! A `RwLock` is used to prevent that:
//!   * Only a single `event_loop` should ever run at one time, the write lock is used to prevent
//!     the `event_loop` to be run twice and prevent any `action_queue` to run.
//!   * Many actions can run at the same time in an `action_queue`, the read lock is used to prevent
//!     actions from being run while an `event_loop` is running.
//!

use crate::actions::ActionError;
use crate::events::MailEvent;
use crate::user_context::events::subscriber::MailEventSubscriber;
use crate::{MailContextResult, MailUserContext};
use proton_action_queue::action::Action;
use proton_action_queue::queue::{ActionOutput, Queue, QueuedActionOutput, QueuedResult};
use proton_core_common::CoreEventSubscriber;
use proton_event_loop::foreground_loop::EventLoop;
use proton_event_loop::provider::Provider;
use proton_event_loop::store::Store;
use proton_event_loop::subscriber::Subscriber;
use proton_event_loop::{Event, EventLoopError};
use std::any::Any;
use std::future::Future;
use std::sync::Weak;
use tokio::sync::RwLock;

pub struct MailUserContextExclusive {
    event_loop: RwLock<EventLoop>,
    action_queue: Queue,
}

impl MailUserContextExclusive {
    /// Create a new `MailUserContextExclusive` containing the given `EventLoop` and `Queue`.
    pub(crate) fn new(event_loop: EventLoop, action_queue: Queue) -> Self {
        Self {
            event_loop: RwLock::new(event_loop),
            action_queue,
        }
    }

    /// Initialize the inner `EventLoop`.
    ///
    /// # Errors
    ///
    /// If initialization fail.
    pub(crate) async fn initialize_event_loop<T: Event + From<<T as Event>::Response>>(
        &self,
        store: &dyn Store,
        provider: &dyn Provider<T>,
    ) -> Result<(), EventLoopError> {
        let event_loop = self.event_loop.write().await;
        event_loop.initialize(store, provider).await
    }

    /// Expose the inner `Queue` while locked into a closure.
    pub(crate) async fn with_queue<'a, F, T>(&'a self, closure: impl FnOnce(&'a Queue) -> F) -> T
    where
        F: Future<Output = T> + 'a,
    {
        let _lock = self.event_loop.read().await;
        closure(&self.action_queue).await
    }

    /// Execute an action immediately.
    ///
    /// # Errors
    ///
    /// Return error if the action could not be executed.
    pub(crate) async fn execute_action<T: Action<Error = ActionError>>(
        &self,
        action: T,
    ) -> MailContextResult<ActionOutput<T>> {
        let _lock = self.event_loop.read().await;
        Ok(self.action_queue.apply_action(action).await?)
    }

    /// Queue an action for later execution.
    ///
    /// # Errors
    ///
    /// Return error if the action could not be queued.
    pub(crate) async fn queue_action<T: Action<Error = ActionError>>(
        &self,
        action: T,
    ) -> MailContextResult<QueuedActionOutput<T>> {
        let _lock = self.event_loop.read().await;
        Ok(self.action_queue.queue_action(action).await?)
    }

    /// Execute exactly one pending action in the queue.
    pub(crate) async fn execute_pending_action(&self) -> MailContextResult<()> {
        let _lock = self.event_loop.read().await;
        let _ = self.action_queue.execute_one().await?;
        Ok(())
    }

    /// Execute all pending actions in the queue.
    pub(crate) async fn execute_pending_actions(&self) -> MailContextResult<usize> {
        let _lock = self.event_loop.read().await;
        Ok(self.action_queue.execute_all().await?)
    }

    /// Register an execution context with the queue.
    ///
    /// Execution context are used by actions to access runtime data.
    ///
    pub(crate) fn register_execution_context<E: Any + Send + Sync + 'static>(
        &self,
        context: Weak<E>,
    ) {
        // No need to lock
        self.action_queue.register_execution_context(context);
    }

    /// Execute all available actions from the queue.
    ///
    /// # Errors
    ///
    /// Returns error if the queued action could not be executed locally or remotely, or if
    /// another thread is currently invoking this function.
    ///
    pub(crate) async fn execute_all(&self) -> QueuedResult<usize> {
        let _lock = self.event_loop.read().await;
        self.action_queue.execute_all().await
    }

    /// Perform one iteration of the event loop, which consists of retrieving the latest events,
    /// publishing it on all the registered subscribers and storing the event id for the next
    /// iteration.
    /// The execution of the loop is aborted on the first error.
    pub(crate) async fn poll_event_loop(
        &self,
        user_context: &MailUserContext,
    ) -> Result<(), EventLoopError> {
        let core_subscriber = CoreEventSubscriber::new(Weak::clone(&user_context.this));
        let mail_subscriber = MailEventSubscriber::new(Weak::clone(&user_context.this));
        let subscribers: [Box<dyn Subscriber<MailEvent>>; 2] =
            [Box::new(core_subscriber), Box::new(mail_subscriber)];

        let event_loop = self.event_loop.write().await;
        event_loop
            .poll(user_context, user_context, &subscribers)
            .await
    }
}
