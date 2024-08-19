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
//!               Here you will find the login and session management.
//!   - [`mail`]: The Proton Mail application's specific functionality, with
//!               everything needed to manage mailboxes, conversations,
//!               messages, and labels.
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

use proton_core_common::datatypes::LocalId as RealLocalId;
use stash::exports::ToSql;
use stash::orm::{Model, ResultsetChange};
use stash::stash::{AgnosticInterface, Interface, StashError};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use tokio::runtime::Runtime;
use tokio::task::JoinError;
use tracing::{debug, warn};

pub mod core;
mod log;
pub mod mail;

uniffi::setup_scaffolding!();

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

/// A handle to a live query.
///
/// This handle can be used to disconnect from the live query.
///
#[derive(Clone, uniffi::Object)]
pub struct WatchHandle {
    /// A flag to indicate if the live query should be stopped.
    stop_flag: Arc<AtomicBool>,
}

impl Default for WatchHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchHandle {
    #[must_use]
    pub fn new() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    #[must_use]
    pub fn should_stop(&self) -> bool {
        self.stop_flag.load(Ordering::SeqCst)
    }
}

#[uniffi::export]
impl WatchHandle {
    /// Disconnect from the live query.
    pub fn disconnect(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

/// Watches records in the database using specific query logic.
///
/// This function calls [`Model::find()`] with the provided query logic and
/// parameters, sets up a queue, and then listens for changes to the result
/// set. When changes occur, they are sent to the queue.
///
/// # Parameters
///
/// * `query_logic`  - The query logic to use for finding the records. This
///                    should be a string that represents the conditions,
///                    ordering, offset, and limit for the query, as may be
///                    required. It can be empty. Note that each part of the
///                    logic is optional — so if conditions are passed, for
///                    instance, the `WHERE` keyword needs to be included.
/// * `params`       - The parameters to use in the query. These should be in
///                    the order they are expected in the query logic, and match
///                    with any expectations set in the query logic.
/// * `check_record` - A function that checks if the record is associated with
///                    the records being watched. This function should return
///                    `true` if the record is associated, and `false` otherwise
///                    and should accept one parameter, which is the record to
///                    check (of type `T`). This is used for `INSERT` and
///                    `UPDATE` change events.
/// * `get_local_id` - A function that returns the local ID of the record. This
///                    function should accept one parameter, which is the record
///                    to get the ID from (of type `T`), and should return the
///                    local ID of the record.
/// * `interface`    - The database interface, i.e. [`Stash`] or [`Tether`], to
///                    use for finding the records.
/// * `callback`     - The callback to use for updates. When the specified
///                    result set changes, the callback will be invoked.
///
/// # Errors
///
/// See [`Stash::find()`].
///
/// # See also
///
/// * [`Model::find()`]
/// * [`params!`](crate::utils::params)
///
pub async fn watch<Q, A, T>(
    query_logic: Q,
    params: Vec<Box<dyn ToSql + Send>>,
    check_record: impl Fn(&T) -> bool + Send + Sync + 'static,
    get_local_id: impl Fn(&T) -> RealLocalId + Send + Sync + 'static,
    interface: &A,
    callback: Arc<Box<dyn LiveQueryCallback>>,
) -> Result<(Vec<T>, Arc<WatchHandle>), StashError>
where
    Q: Into<String> + Send,
    A: Into<AgnosticInterface> + Interface,
    T: Model<IdType = RealLocalId>,
{
    let (sender, receiver) = flume::unbounded::<ResultsetChange<T, RealLocalId>>();
    let results = T::find(query_logic, params, interface.stash(), Some(sender)).await?;
    // Unwrapping is safe here, as we will always have the local ID
    #[allow(clippy::redundant_closure)]
    let mut ids = results.iter().map(|m| get_local_id(m)).collect::<Vec<_>>();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = Arc::clone(&stop_flag);

    spawn_async(async move {
        while let Ok(change) = receiver.recv_async().await {
            if stop_flag_clone.load(Ordering::SeqCst) {
                debug!("Stop flag set, stopping watch");
                break;
            }
            match change {
                ResultsetChange::Inserted(record) => {
                    if check_record(&record) {
                        debug!("Received new record for watched set");
                        ids.push(get_local_id(&record));
                        callback.on_update();
                    } else {
                        debug!("Received new record not related to set");
                    }
                }
                ResultsetChange::Updated(record) => {
                    if check_record(&record) {
                        debug!("Received updated record for watched set");
                        callback.on_update();
                    } else {
                        debug!("Received updated record not related to set");
                    }
                }
                ResultsetChange::Deleted(record_id) => {
                    if ids.contains(&record_id) {
                        debug!("Received deleted record for watched set");
                        callback.on_update();
                    } else {
                        debug!("Received deleted record not related to set");
                    }
                }
                _ => {
                    warn!("Received unknown change type");
                }
            };
        }
    });

    Ok((
        results.into_iter().map(Into::into).collect(),
        Arc::new(WatchHandle { stop_flag }),
    ))
}

/// Get the async runtime.
fn async_runtime() -> &'static Runtime {
    static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("Failed to init runtime")
    });

    &RUNTIME
}

/// Spawn an async function on the runtime.
fn spawn_async<T, F>(future: F) -> tokio::task::JoinHandle<T>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    async_runtime().spawn(future)
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
