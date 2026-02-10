//! This crate provides abstraction to spawn cancellable and pausable future tasks.
//!
//! # Why is this necessary?
//!
//! First we have one specific implementation which defines how all our tasks are spawned. This
//! is important to abstract platform specific details when we eventually port to webassembly.
//!
//! Secondly, certain platforms like iOS have strict requirements on what should happen once the
//! application is dismissed or is done running background tasks. We have to ensure that we pause
//! all background work and allow certain import work (e.g: Sqlite Transactions) so that they
//! release the resources that will otherwise cause the OS to terminate the process.
//!
//! # Pausing & Resuming
//!
//! To pause and resume execution of futures, simply call the respective methods on the service.
//!
//! ```no_run
//! use proton_task_service::TaskService;
//! use tokio::runtime::Handle;
//! # async fn foo() {
//!     let service = TaskService::new(Handle::current()).unwrap();
//!
//!     service.spawn(async move {
//!        // Do something with this future.
//!     });
//!
//!     // Pause future execution.
//!     service.pause();
//!
//!     // Resume future execution.
//!     service.resume();
//! # }
//!
//! ```
//!
//! # Not Pausable Futures
//!
//! Not pausable futures should be used to cover critical areas that should always finish even if
//! the parent future is paused.
//!
//! ```no_run
//! use proton_task_service::{IntoNonPausableFuture, TaskService};
//! use tokio::runtime::Handle;
//! # async fn foo() {
//!     let service = TaskService::new(Handle::current()).unwrap();
//!
//!     service.spawn(async move {
//!         // This future will pause.
//!     });
//!
//!     service.spawn(async move {
//!         // This future will never pause.
//!     }.into_non_pausable());
//!
//!     // Pause future execution.
//!     service.pause();
//!
//!     // Resume future execution.
//!     service.resume();
//! # }
//!
//! ```
//!
mod service;
mod spawn;

pub use self::service::*;
pub use self::spawn::*;
