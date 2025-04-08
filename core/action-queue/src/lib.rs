//! This crate provides the backbone to executes actions optimistically on a local data set and
//! then apply them on a remote server.
//!
//! To achieve this each action needs to implement the [`Action`] and the [`Handler`] trait.
//!
//! Actions can then be executed immediately or queued for future execution on a [`Queue`] with
//! default or custom [`Metadata`].
//!
//! The [`Metadata`] allows the use to override priority, delay execution, assign dependency
//! chains, among others.
//!
//! Actions that are stored in the queue are serialized and versioned. A [`VersionConverter`] can
//! be assigned to each action to update the serialized data from previous versions.
//!
//! Finally, all actions need to be registered with [`Factory`] so they can be deserialized
//! from the queue.
//!
//! # Example
//!
//! ```
//! use std::future::Future;
//! use std::sync::Arc;
//! use serde::{Deserialize, Serialize};
//! use proton_action_queue::action::{Action, DefaultVersionConverter, Factory, Handler, ActionId, Metadata, Priority, Type, WriterGuardError, WriterGuard};
//! use proton_action_queue::queue::{ActionRemoteOutput, Queue};
//! use stash::stash::{Stash, Bond};
//!
//! #[derive(Serialize, Deserialize)]
//! struct MyAction {
//!    value:u32
//! }
//!
//! #[derive(Debug,thiserror::Error, PartialEq)]
//! enum MyActionError{
//!     #[error("Foo")]
//!     Foo,
//!     #[error("Request")]
//!     Request,
//!     #[error("WriterGuardExpired")]
//!     WriterGuardExpired,
//! }
//!
//! impl proton_action_queue::action::Error for MyActionError {
//!     fn is_network_failure(&self) -> bool {
//!         Self::Request == *self
//!     }
//!
//!     fn is_writer_guard_expired(&self) -> bool {
//!        Self::WriterGuardExpired == *self
//!     }
//!}
//!
//! impl From<WriterGuardError> for MyActionError {
//!    fn from(value: WriterGuardError) -> Self {
//!        if matches!(value, WriterGuardError::Expired) {
//!            Self::WriterGuardExpired
//!        } else {
//!            Self::Foo
//!        }
//!    }
//! }
//!
//! impl Action for MyAction {
//!     const TYPE: Type = Type("my_action");
//!     const VERSION: u32 = 0;
//!     const PRIORITY: Priority = Priority::Normal;
//!     type VersionConverter = DefaultVersionConverter<Self>;
//!     type Handler = MyActionHandler;
//!     type RemoteOutput = ();
//!     type LocalOutput = ();
//!     type Error = MyActionError;
//!     type Context = ();
//! }
//!
//! #[derive(Default)]
//! struct MyActionHandler{}
//!
//! impl Handler for MyActionHandler {
//!     type Action = MyAction;
//!     type Context = ();
//!
//!     async fn apply_local(&self, action_id:ActionId, ctx: &Self::Context, action: &mut Self::Action, bond: &Bond<'_>) -> Result<(), <Self::Action as Action>::Error> {
//!         todo!()
//!     }
//!
//!     async fn revert_local(&self, action_id:ActionId, ctx: &Self::Context, action: &mut Self::Action, bond: &Bond<'_>) -> Result<(),<Self::Action as Action>::Error> {
//!         todo!()
//!     }
//!
//!     async fn apply_remote(&self, action_id:ActionId, ctx: &Self::Context, action: &mut Self::Action, guard: WriterGuard<'_>) -> Result<<Self::Action as Action>::RemoteOutput,<Self::Action as Action>::Error> {
//!         todo!()
//!     }
//! }
//!
//! async fn example() {
//!     // Create stash instance.
//!     let stash = stash::stash::Stash::new(None).unwrap();
//!     // create queue.
//!     let queue = Queue::new(stash).await.unwrap();
//!     // register action.
//!     queue.register::<MyAction>().unwrap();
//!     // create executor
//!     let executor = queue.new_executor();
//!     // Execute action immediately
//!     let queued_id = queue.queue_action(MyAction{value:10}).await.unwrap().id;
//!
//!     // Queue an action which depends on another action.
//!     let queued_id2= queue.queue_action_with_metadata(MyAction{value:30},
//!         Metadata::builder()
//!             .with_dependency(queued_id)
//!             .with_debug_string("To be or not to be")
//!             .build()
//!     ).await.unwrap();
//!
//!     // Flush all available actions.
//!     executor.execute_all().await.unwrap();
//! }
//!
//! ```
//!
//! [`Action`]: action::Action
//! [`VersionConverter`]: action::VersionConverter
//! [`Metadata`]: action::Metadata
//! [`Handler`]: action::Handler
//! [`Queue`]: queue::Queue
//! [`Factory`]: action::Factory

pub mod action;
pub mod db;
pub mod observers;
pub mod queue;

#[cfg(any(test, debug_assertions))]
pub mod tests {
    #[path = "../tests/common.rs"]
    pub mod common;
}
