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
//! to be created in order to access all the user settings and labels. This
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

use proton_core_common::watch_handle::WatchHandle as RealWatchHandle;
use proton_mail_common::datatypes::SearchOptions as RealSearchOptions;
use proton_task_service::Spawner;
use sqlite_watcher::watcher::DropRemoveTableObserverHandle;
use std::sync::Arc;
use tokio::task::JoinHandle;
use uniffi_runtime::{async_runtime, async_runtime_slim, uniffi_async};

// Reexport renamed items from the `uniffi` crate.
pub use uniffi::{Enum as UniffiEnum, Record as UniffiRecord};

#[macro_use]
extern crate uniffi_macros;

#[macro_use]
pub mod errors;

pub mod core;
mod log;
pub mod mail;
pub mod version;

#[cfg(target_os = "android")]
pub mod jni;

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
    pub fn new(watch_handle: DropRemoveTableObserverHandle, task_handle: &JoinHandle<()>) -> Self {
        Self(RealWatchHandle::new(watch_handle, task_handle))
    }
}

#[uniffi_export]
impl WatchHandle {
    pub fn disconnect(self: Arc<Self>) {
        self.0.disconnect();
    }
}

pub fn watch_channel_inner<T: Send + 'static>(
    ctx: &impl Spawner,
    channel: flume::Receiver<T>,
    callback: impl Fn() + Send + Sync + 'static,
) -> JoinHandle<()> {
    // use a one-shot channel to act as an early exit strategy.
    ctx.spawn_task(async move {
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

#[macro_export]
macro_rules! declare_live_query_tagger {
    ($name:ident) => {
        struct $name;

        impl $name {
            #[allow(clippy::unit_arg)]
            #[inline(never)]
            #[allow(dead_code)]
            fn tag_sync(cb: &dyn $crate::LiveQueryCallback) {
                std::hint::black_box(cb.on_update());
            }

            #[allow(clippy::unit_arg)]
            #[inline(never)]
            #[allow(dead_code)]
            fn tag_async(
                cb: &dyn $crate::AsyncLiveQueryCallback,
            ) -> impl Future<Output = ()> + Send + '_ {
                async {
                    std::hint::black_box(cb.on_update().await);
                }
            }

            #[must_use]
            #[allow(dead_code)]
            pub fn watch_channel_async(
                ctx: &impl ::proton_task_service::Spawner,
                handle: ::stash::stash::WatcherHandle,
                callback: Arc<dyn $crate::AsyncLiveQueryCallback>,
            ) -> Arc<$crate::WatchHandle> {
                let ::stash::stash::WatcherHandle {
                    receiver, handle, ..
                } = handle;

                let task_handle = ctx.spawn_task(async move {
                    while receiver.recv_async().await.is_ok() {
                        Self::tag_async(callback.as_ref()).await;
                    }
                });

                Arc::new($crate::WatchHandle::new(handle, &task_handle))
            }

            #[must_use]
            #[allow(dead_code)]
            pub fn watch_channel(
                ctx: &impl ::proton_task_service::Spawner,
                handle: ::stash::stash::WatcherHandle,
                callback: Box<dyn $crate::LiveQueryCallback>,
            ) -> Arc<$crate::WatchHandle> {
                let task_handle = $crate::watch_channel_inner(ctx, handle.receiver, move || {
                    Self::tag_sync(callback.as_ref());
                });

                Arc::new($crate::WatchHandle::new(handle.handle, &task_handle))
            }
        }
    };
}

#[macro_export]
macro_rules! watch_table {
    ($tag: tt, $ctx:expr, $spawner:expr, $callback:expr, $watch_fn:expr) => {{
        let watcher_handle = $watch_fn($ctx)
            .await
            .inspect_err(|err| error!("Error while getting user_context: {err:?}"))
            .map_err(|_| ProtonError::Unexpected(UnexpectedError::Database))?;
        let watch_handle = $tag::watch_channel_async($spawner.as_ref(), watcher_handle, $callback);
        Ok(watch_handle)
    }};
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

#[uniffi::export]
fn make_me_crash() {
    panic!("I will crash");
}
