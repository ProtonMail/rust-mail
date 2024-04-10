#![allow(clippy::pedantic)] // this crate will get refactored.
//! This crate provides an implementation of queue where actions can be recorded which modify the
//! local proton application. Queued action can then be executed, when appropriate by the
//! application in the same or different process.
//!
//! # Flow of Execution
//! The [`Action`]s themselves fully support rollback and reconciliation, they only need to abide by
//! a simple set of rules which are described in the sections below.
//!
//! ## Local State Modification
//! When actions are queued, they are serialized to a table where they will be executed at the
//! application's convenience. After the action has been recorded local state changes can be applied
//! via an implementation of ([`LocalActionHandler`]).
//!
//! ## Pre-Execution Validation
//! Since an arbitrary number of actions can be queued before a given action is executed, the local
//! state can be in a constants state of flux. The action should validate if the local state still
//! matches the outcome of the action ([`RemoteActionHandler::validate_local`]). If this is no longer
//! the case the action can be skipped entirely by returning [`ActionLocalValidationResult::Invalid`]
//! or by reducing the set of changes to only valid operations and then returning:
//! [`ActionLocalValidationResult::Valid`].
//!
//! If the former is returned, the action is skipped **and [`RemoteActionHandler::revert_local`] is
//! not called**. Consider the following set of queued actions with M1 currently in F0:
//!
//! * Move M1 to F1
//! * Move M1 to F2
//! * Move M1 to F3
//!
//! Locally, M1 is now in F3, so the local check of the first action should fail, since M1 is no
//! longer in F1. However, if we were to roll back our local changes, we would move M1 from F3 back
//! to F0 and the final action would also not execute anymore.
//!
//! ## Remote Execution Succeeds
//! The action is now executed on the remote ([`RemoteActionHandler::apply_remote`]) and, if
//! successful, the change is considered permanent. The action can now perform additional
//! bookkeeping to ensure correct state rollback as well.
//!
//! Consider the following set of queued actions with M1 currently in F0:
//!
//! * Move M1 to F1 - On failure move to F0
//! * Move M1 to F2 - On failure move to F0
//! * Move M1 to F3 - On failure move to F0
//!
//! With what we know so far, the first 2 actions are skipped an action 3 is executed. S
//! If after this point a new action is queued, reverting the action should now move the message
//! back to F3 rather than F4.
//!
//! This bookkeeping will depend on the requirement specific to the action in question and the data
//! being manipulated. See the tests for an example.
//!
//! ## Remote Execute Fails
//! If the action failed to be applied remotely, it is expected that local state is reverted back
//! to before the action was applied ([`RemoteActionHandler::revert_local`]).
//!
//! # Registering Actions
//! After implementing the [`Action`], [`LocalActionHandler`] and [`RemoteActionHandler`] traits for
//! your action, implement the [`ActionFactoryInstance`] trait for your action factory and register
//! it with an [`ActionFactory`].
//!
//! # Example
//! ```rust,ignore
//! use std::any::Any;
//! use proton_action_queue::{Action, ActionFactory, ActionFactoryInstance, ActionFactoryInstanceError, ActionId, ActionLocalValidationResult, ActionPriority, ActionQueue, ActionResult, AlwaysErrorSessionProvider, define_action_id, LocalActionHandler, RemoteActionHandler, SessionProvider, StoredAction};
//! use serde::{Deserialize,Serialize};
//! use proton_sqlite3::rusqlite::Transaction;
//! use proton_sqlite3::{SqliteConnectionPool, SqliteMode};
//!
//! define_action_id!(MY_ACTION_ID, "39ee92ac-f6ce-4aa1-9f25-695d6ca393af");
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct MyAction {
//! // struct members
//! }
//!
//! const MY_ACTION_VERSION:u32 = 1;
//!
//! impl Action for MyAction {
//!     const ID:ActionId = MY_ACTION_ID;
//!     const VERSION: u32 = MY_ACTION_VERSION;
//! }
//!
//! struct MyActionLocalHandler<'a>{
//!     action: &'a MyAction
//! // Interfaces/adapters/members/etc...
//! }
//!
//! impl<'a> LocalActionHandler for MyActionLocalHandler<'a> {
//!     fn apply_local(&mut self) -> ActionResult<()> {
//!         todo!()
//!     }
//! }
//!
//! struct MyActionRemoteHandler {
//!     action: MyAction
//!     // Interfaces/adapters/members/etc...
//! }
//!
//! impl RemoteActionHandler for MyActionRemoteHandler {
//!    fn revert_local(&mut self) -> ActionResult<()> {
//!         todo!()
//!     }
//!
//!     fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
//!         todo!()
//!     }
//!
//!      fn apply_remote(&mut self) -> ActionResult<()> {
//!         todo!()
//!     }
//! }
//!
//! #[derive(Debug)]
//! struct MyActionFactory {}
//!
//! impl ActionFactoryInstance for MyActionFactory {
//!     fn action_id(&self) -> &'static ActionId {
//!         &MyAction::ID
//!     }
//!
//!     fn local_handler<'r, 't: 'r>(&self, action: &'r dyn Any, _tx: &'r mut Transaction<'t>) -> Result<Box<dyn LocalActionHandler + 'r>, ActionFactoryInstanceError> {
//!         let Some(action) = action.downcast_ref::<MyAction>() else {
//!             return Err(ActionFactoryInstanceError::InvalidType(
//!                 action.type_name(),
//!                 std::any::TypeId::of::<MyAction>(),
//!             ));
//!         };
//!
//!         Ok(Box::new(MyActionLocalHandler {
//!             action,
//!         }))
//!     }
//!
//!     fn remote_handler<'r, 't: 'r>(&'r self, action: &StoredAction, _tx: &'r mut Transaction<'t>, _session_provider: &dyn SessionProvider) -> Result<Box<dyn RemoteActionHandler + 'r>, ActionFactoryInstanceError> {
//!         if action.version != MY_ACTION_VERSION {
//!             return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
//!         }
//!         let action = action.deserialize::<MyAction>()?;
//!         Ok(Box::new(MyActionRemoteHandler {action}))
//!     }
//! }
//!
//! // Note: prefer file based SqliteMode for better performance.
//! let pool = SqliteConnectionPool::new(SqliteMode::InMemory);
//! let mut factory = ActionFactory::new();
//! factory.register(Box::new(MyActionFactory{})).unwrap();
//!
//! // Replace with usable implementation
//! let session_provider = Box::new(AlwaysErrorSessionProvider{});
//! let connection = pool.acquire().unwrap();
//! let mut queue = ActionQueue::new(connection, session_provider, factory).unwrap();
//!
//! let action = MyAction{};
//! queue.queue_action(&action, ActionPriority::Normal).unwrap();
//!
//! // Sometime later in a different thread/location/process/etc...
//! queue.consume_pending().unwrap();
//!
//! ```
//!
mod action;
mod providers;
mod queue;
mod store;

pub use action::*;
pub use providers::*;
pub use queue::*;
pub use store::*;
