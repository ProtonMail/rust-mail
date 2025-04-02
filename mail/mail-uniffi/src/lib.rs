#![allow(clippy::doc_markdown)]

//! UniFFI bindings for Proton Mail.
//!
//! # Getting started
//!
//! When a client using the FFI bindings starts, it needs to establish a
//! connection to the underlying subsystem via the Rust libraries. This is done
//! by creating a new context/session object, which needs to be kept alive for
//! the lifetime of the application in order to maintain access to resources
//! such as the local database and continue the processing of background tasks.
//!
//! TODO: Update the section below once contexts/sessions are fully-unified
//!
//! This is done by creating a [`MailSession`](mail::MailSession), which is a
//! manual process i.e. the client should call a constructor such as
//! [`MailSession::new()`](mail::MailSession::create()) and pass in the required
//! information. After this, a [`MailUserSession`](mail::MailUserSession) needs
//! to be created in other to access all the user settings and labels. This
//! second step is done automatically when establishing an authentication
//! context.
//!
//! # Features and concepts
//!
//! ## Authentication
//!
//! There are two routes in to establishing an authentication context:
//!
//!   1. You can obtain one by performing fresh login using [`new_login_flow()`](mail::MailSession::new_login_flow()),
//!      or
//!   2. You can use an existing session with
//!      [`user_context_from_session()`](mail::MailSession::user_context_from_session()).
//!
//! Once logged in, you will have access to all the labels and user-related
//! settings.
//!
//! TODO: Expand this section with step-by-step instructions
//!
//! ## Mailboxes
//!
//! In the jargon used by Proton Mail, a "mailbox" is a view onto a "thing" that
//! "contains" messages/conversations. This might be a folder or label, and so
//! the term "mailbox" does not align with the entire account as might be
//! traditionally expected.
//!
//! To access the conversations and messages you need to create a [`Mailbox`](mail::Mailbox)
//! for the active label. You then need to create a live query for the
//! conversations of that mailbox with [`new_conversation_live_query()`](mail::Mailbox::new_conversation_live_query()).
//!
//! ## Actions
//!
//! Any data changes will generate actions that are queued for execution at a
//! time that makes sense for the client.
//!
//! To execute any pending actions immediately, call
//! [`execute_pending_action()`](mail::MailUserSession::execute_pending_action())
//! to execute one action, or [`execute_pending_actions()`](mail::MailUserSession::execute_pending_actions())
//! to execute all pending actions.
//!
//! # Layout
//!
//! The FFI bindings are split into a number of modules, each of which
//! corresponds to a different area of functionality, broadly correlating with
//! the Rust internals, but that is not mandatory. Each overall FFI "package" is
//! focused on a specific Proton application, with this one being Proton Mail.
//! Each package contains everything needed to run and operate that product and
//! manage its resources and operations. Therefore, there are a number of pieces
//! of functionality that are common or "core", and these are shared so that all
//! packages present those fundamental components (a good example being login).
//!
//! Broadly, the structure is as follows:
//!
//!   - [`core`]: Core functionality that is common to all Proton applications.
//!     Here you will find the login and session management.
//!   - [`mail`]: The Proton Mail application's specific functionality, with
//!     everything needed to manage mailboxes, conversations,
//!     messages, and labels.
//!
//! In addition to particular features, there are also concepts that are
//! established in core and then extended in the product, such as actions,
//! contexts/sessions, and events.
//!
//! # TODO: Add more information on contexts/sessions when fully-unified
//! # TODO: Add more information on events when new event system is in place
//!
//! Finally, there are also small utilities that are exposed under their own
//! module namespaces.
//!
//! # TODO: Check the above after the facade is complete, in case it changes
//!
//! ## Live queries
//!
//! Live queries are a way to observe data and be notified when it changes. This
//! is useful for keeping the client's view of the data up-to-date without
//! needing to poll for changes.
//!
//! Where live query functionality is provided (through various `watch`
//! functions), they accept a callback. This callback will be invoked whenever
//! the data being watched changes. The data being watched is the result of an
//! initial query, the rows from which are returned when the live query is first
//! instigated. This is why this mechanism is referred to as "live queries" —
//! because there is an initial query, and then any changes to the generated
//! result set will trigger a notification.
//!
//! The live query functions return a data structure which contains the results
//! plus a handle to the live query observer. These are separate in nature, so
//! that the results can be used and dropped, and the handle retained. The
//! handle is a [`WatchHandle`], which has a [`disconnect()`](WatchHandle::disconnect())
//! method that can be called to stop observing changes to the live query result
//! set.
//!
//! # Rust internals
//!
//! The actual internal structure and operations of the wider Rust libraries are
//! hidden away, and not exposed directly through the FFI bindings. Instead, a
//! facade is in place, which operates as a translation layer between the public
//! interface and private internals. This is for a number of reasons:
//!
//!   - It allows full independence so that changes can be made on either side
//!     without concern. This means that the Rust team can change and maintain
//!     the internal codebase as seen fit for Rust development, and meanwhile
//!     the exported functionality can be amended to suit the needs and desires
//!     of the teams consuming the interface.
//!
//!   - It allows for a more stable interface. The FFI bindings are a contract
//!     between the Rust libraries and the client, and by keeping the internals
//!     hidden, the contract is more stable and less likely to change.
//!
//!   - It allows Rust development to take place using all Rust features without
//!     concern, as any incompatibilities with UniFFI will be handled by the
//!     facade.
//!
//! This facade is a parallel of the approach used when communicating with the
//! Proton REST API from the Rust libraries, which is in place for similar
//! reasons, and both are expressions of a driver or adaptor pattern.
//!
//! *Note to Rust developers: The documentation in this crate is aimed primarily
//! at the client developers, who will be using the exposed bindings. All
//! functionality herein is lightweight, and for the purpose of translation
//! only, and so for any internal information please refer to the internal Rust
//! crates that are the subject of the translations.*
//!

use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use stash::stash::WatcherHandle;
// Reexport renamed items from the `uniffi` crate.
pub use uniffi::{Enum as UniffiEnum, Record as UniffiRecord};

use proton_core_common::watch_handle::WatchHandle as RealWatchHandle;
use proton_mail_common::datatypes::SearchOptions as RealSearchOptions;
use proton_mail_common::{MailContext, MailUserContext};
use proton_task_service::{AsyncTaskResult, TaskSpawner};
use std::future::Future;
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;
use tokio::task::JoinError;
use tokio::task::JoinHandle;

#[macro_use]
extern crate proton_uniffi_macros;

pub mod core;
#[macro_use]
pub mod errors;
mod log;
pub mod mail;

#[cfg(target_os = "android")]
pub mod tls;
pub mod version;

uniffi::setup_scaffolding!("proton_mail_uniffi");

/// A callback interface for live queries.
///
/// This interface is used to notify the client when observed data has been
/// updated.
///
#[uniffi::export(callback_interface)]
pub trait LiveQueryCallback: Send + Sync {
    /// Notify the client that the observed data has been updated.
    ///
    /// This method is called when the observed data has been updated. It does
    /// not provide any information about the update, but the client can use
    /// this as a signal to refresh its view of the data.
    ///
    fn on_update(&self);
}

/// An async callback interface for live queries.
///
/// This interface is used to notify the client when observed data has been
/// updated.
///
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait AsyncLiveQueryCallback: Send + Sync {
    /// Notify the client that the observed data has been updated.
    ///
    /// This method is called when the observed data has been updated. It does
    /// not provide any information about the update, but the client can use
    /// this as a signal to refresh its view of the data.
    ///
    async fn on_update(&self);
}

/// A handle to a live query.
///
/// This handle can be used to disconnect from the live query.
///
#[derive(uniffi::Object)]
pub struct WatchHandle(RealWatchHandle);

impl WatchHandle {
    #[must_use]
    pub fn new(
        watch_handle: DropRemoveTableObserverHandle,
        task_handle: &JoinHandle<AsyncTaskResult<()>>,
    ) -> Self {
        Self(RealWatchHandle::new(watch_handle, task_handle))
    }
}

#[uniffi_export]
impl WatchHandle {
    pub fn disconnect(self: Arc<Self>) {
        self.0.disconnect();
    }
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Get the async runtime.
///
/// # Using both [`async_runtime`] and [`async_runtime_slim`]
///
/// Both functions are competing for initializing common runtime. It means, that whichever function
/// is called first, it's configuring the runtime, and all other following calls are only
/// returning already initialized runtime.
///
/// ## Examples
///
/// ```ignore
/// let runtime1 = async_runtime();
/// let runtime2 = async_runtime_slim();
/// // This runtime2 is a static reference to FULL runtime using all possible cores,
/// // because `async_runtime()` was called first
/// ```
///
/// ```ignore
/// let runtime1 = async_runtime_slim();
/// let runtime2 = async_runtime();
/// // This runtime2 is a static reference to SLIM runtime using limited number of cores,
/// // because `async_runtime_slim()` was called first
/// ```
///
/// # Panics
///
/// This function may panic if Tokio fails to init async runtime
///
#[must_use]
pub fn async_runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("Failed to init runtime")
    })
}

/// Get slimmer version of the async runtime.
///
/// Comparing to [`async_runtime`] this takes very limited number of threads.
/// It is to enable Rust SDK in apps with limited amount of memory.
///
/// # Using both [`async_runtime`] and [`async_runtime_slim`]
///
/// Both functions are competing for initializing common runtime. It means, that whichever function
/// is called first, it's configuring the runtime, and all other following calls are only
/// returning already initialized runtime.
///
/// ## Examples
///
/// ```ignore
/// let runtime1 = async_runtime();
/// let runtime2 = async_runtime_slim();
/// // This runtime2 is a static reference to FULL runtime using all possible cores,
/// // because `async_runtime()` was called first
/// ```
///
/// ```ignore
/// let runtime1 = async_runtime_slim();
/// let runtime2 = async_runtime();
/// // This runtime2 is a static reference to SLIM runtime using limited number of cores,
/// // because `async_runtime_slim()` was called first
/// ```
///
/// # Panics
///
/// This function may panic if Tokio fails to init async runtime
///
#[must_use]
pub fn async_runtime_slim() -> &'static Runtime {
    // Those numbers are arbitrary
    //
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .max_blocking_threads(4)
            .enable_io()
            .enable_time()
            .build()
            .expect("Failed to init runtime")
    })
}

/// Spawn an async function on the runtime.
fn spawn_async<S, T, F>(ctx: impl AsRef<S>, future: F) -> JoinHandle<AsyncTaskResult<T>>
where
    S: AsyncSpawnable,
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    ctx.as_ref().spawn(future)
}

/// Run an async function on the Tokio runtime.
async fn uniffi_async<T, E, F>(future: F) -> Result<T, E>
where
    E: Send + From<JoinError> + 'static,
    T: Send + 'static,
    F: Future<Output = Result<T, E>> + Send + 'static,
{
    let handle = async_runtime().spawn(future);
    handle.await?
}

/// Abstraction trait so we can reference either [`MailContext`] or [`MailUserContext`]
/// when spawning tasks.
pub trait AsyncSpawnable {
    fn spawn<F>(&self, future: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        F: Future + Send + 'static,
        <F as Future>::Output: Send + 'static;
}

impl AsyncSpawnable for MailUserContext {
    fn spawn<F>(&self, future: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        F: Future + Send + 'static,
        <F as Future>::Output: Send + 'static,
    {
        self.spawn_with::<UniffiTaskSpawner, _>(future)
    }
}

impl AsyncSpawnable for MailContext {
    fn spawn<F>(&self, future: F) -> JoinHandle<AsyncTaskResult<F::Output>>
    where
        F: Future + Send + 'static,
        <F as Future>::Output: Send + 'static,
    {
        self.spawn_with::<UniffiTaskSpawner, _>(future)
    }
}

/// Task spawner that works over the runtime managed by us.
struct UniffiTaskSpawner;

impl TaskSpawner for UniffiTaskSpawner {
    fn spawn<F>(f: F) -> JoinHandle<F::Output>
    where
        F::Output: Send + 'static,
        F: Future + Send + 'static,
    {
        async_runtime().spawn(f)
    }
}

/// Watch a notification channel for changes and trigger the callback
/// once a message has been received.
///
#[must_use]
pub fn watch_channel<S: AsyncSpawnable>(
    ctx: impl AsRef<S>,
    handle: WatcherHandle,
    callback: Box<dyn LiveQueryCallback>,
) -> Arc<WatchHandle> {
    let task_handle = watch_channel_inner(ctx, handle.receiver, move || {
        callback.on_update();
    });

    Arc::new(WatchHandle::new(handle.handle, &task_handle))
}

fn watch_channel_inner<S: AsyncSpawnable, T: Send + 'static>(
    ctx: impl AsRef<S>,
    channel: flume::Receiver<T>,
    callback: impl Fn() + Send + Sync + 'static,
) -> JoinHandle<AsyncTaskResult<()>> {
    // use a one-shot channel to act as an early exit strategy.
    spawn_async(ctx, async move {
        let callback = Arc::new(callback);
        loop {
            if channel.recv_async().await.is_err() {
                return;
            }

            let callback = callback.clone();
            let callback = move || callback();
            _ = async_runtime().spawn_blocking(callback).await;
        }
    })
}

/// Watch a notification channel for changes and trigger the callback
/// once a message has been received.
///
#[must_use]
pub fn watch_channel_async<S: AsyncSpawnable>(
    ctx: impl AsRef<S>,
    handle: WatcherHandle,
    callback: Arc<dyn AsyncLiveQueryCallback>,
) -> Arc<WatchHandle> {
    let WatcherHandle {
        receiver, handle, ..
    } = handle;

    let task_handle = spawn_async(ctx, async move {
        while receiver.recv_async().await.is_ok() {
            callback.on_update().await;
        }
    });

    Arc::new(WatchHandle::new(handle, &task_handle))
}

/// Search options for pagination
#[derive(uniffi::Record)]
pub struct PaginatorSearchOptions {
    /// Keywords to use in search.
    pub keywords: Option<String>,
}

impl From<PaginatorSearchOptions> for RealSearchOptions {
    fn from(search_options: PaginatorSearchOptions) -> Self {
        RealSearchOptions {
            keywords: search_options.keywords,
        }
    }
}

pub trait MapIntoResult<T, E> {
    fn map_into<T1, E1>(self) -> Result<T1, E1>
    where
        T: Into<T1>,
        E: Into<E1>;
}

impl<T, E> MapIntoResult<T, E> for Result<T, E> {
    fn map_into<T1, E1>(self) -> Result<T1, E1>
    where
        T: Into<T1>,
        E: Into<E1>,
    {
        self.map(Into::into).map_err(Into::into)
    }
}
