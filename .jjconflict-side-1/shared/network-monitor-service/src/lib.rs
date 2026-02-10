//! Network monitor service which observes network activity to determine whether we
//! are currently online or offline with a mix of request analysis and external feedback (e.g.: Os
//! level events).
//!
//!
//! For each new connection, a [`ConnectionMonitor`] should be created from an
//! [`NetworkMonitorService`]. The connection should notify the monitor of new changes.
//!
//! To observe the status of the network one should, preferably use the [`NetworkStatusObserver`]
//! which can be obtained from either the [`ConnectionMonitor`] or the [`NetworkService`].
//!
//! ## Immediate Checks
//!
//! In some cases it may be required to perform a quicker check rather than relying on the background
//! checker which will run very infrequently. For these times you can use
//! [`NetworkMonitorService::check_now()`].
//!
//! Note that these are throttled in according to the `interval` parameter of [`ImmediateConfig`].
//!
//! ## OS Level updates
//!
//! To communicate an OS level update use [`NetworkMonitorService::update_os_network_status()`].
//!
//! # Example
//!
//! ```rust
//! use std::sync::Arc;
//! use proton_network_monitor_service::{Config, NetworkMonitorService, NetworkStatusObserver, OnlineTester, RequestNetworkStatus};
//!
//! async fn monitor(tester:Arc<dyn OnlineTester>) {
//!     let config = Config::default();
//!     let mut service = NetworkMonitorService::new(config);
//!     let spawner = proton_task_service::Tokio::spawner();
//!
//!     service.start(&spawner, tester).unwrap();
//!
//!     // for each connection
//!     let connection_monitor = service.new_connection_monitor();
//!     connection_monitor.update_request_status(RequestNetworkStatus::Online);
//!
//!     // observer changes
//!     let mut observer = service.network_status_observer();
//!     observer.wait_until_online().await;
//! }
//!
//! ```
//!

mod connection_monitor;
mod service;

#[cfg(feature = "muon")]
pub mod muon;

pub use connection_monitor::*;
pub use service::*;
use std::time::Duration;

use tokio::sync::watch;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RequestNetworkStatus {
    Online,
    ServerUnreachable,
    Offline,
}

impl RequestNetworkStatus {
    #[must_use]
    pub fn is_online(&self) -> bool {
        *self == RequestNetworkStatus::Online
    }

    #[must_use]
    pub fn is_offline(&self) -> bool {
        *self != RequestNetworkStatus::Online
    }

    fn combine(os_status: OsNetworkStatus, request_status: RequestNetworkStatus) -> Self {
        if os_status == OsNetworkStatus::Offline {
            RequestNetworkStatus::Offline
        } else {
            request_status
        }
    }
}

#[derive(Clone)]
pub struct NetworkStatusObserver {
    receiver: watch::Receiver<RequestNetworkStatus>,
}

impl NetworkStatusObserver {
    fn new(receiver: watch::Receiver<RequestNetworkStatus>) -> Self {
        Self { receiver }
    }

    pub async fn wait_until_online(&mut self) {
        // `wait_for()` returns `Err` if the channel's tx has died - this
        // shouldn't be the case here, because the channel is allowed to die
        // only after the *last* instance of status watcher is dropped, and we
        // know at least one instance must be alive as it's held within `self`.
        //
        // If this logic becomes violated, the worst that can happen is that
        // this function returns even if the network connection is actually
        // offline. This is alright, because listening on network status is
        // advisory anyway - the caller is supposed to handle potential network
        // problems on their side one way or another.
        let _ = self
            .receiver
            .wait_for(RequestNetworkStatus::is_online)
            .await;
    }

    #[must_use]
    pub fn is_online(&self) -> bool {
        self.receiver.borrow().is_online()
    }

    #[must_use]
    pub fn status(&self) -> RequestNetworkStatus {
        *self.receiver.borrow()
    }

    pub async fn wait_for_change(&mut self) -> RequestNetworkStatus {
        // `changed()` returns `Err` if the channel's tx has died - this
        // shouldn't be the case here, because the channel is allowed to die
        // only after the *last* instance of status watcher is dropped, and we
        // know at least one instance must be alive as it's held within `self`.
        //
        // If this logic becomes violated, the worst that can happen is that
        // this function returns even if the network connection is actually
        // offline. This is alright, because listening on network status is
        // advisory anyway - the caller is supposed to handle potential network
        // problems on their side one way or another.
        let _ = self.receiver.changed().await;
        *self.receiver.borrow()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum OsNetworkStatus {
    Online,
    Offline,
}

impl OsNetworkStatus {
    #[must_use]
    pub fn is_online(&self) -> bool {
        self == &OsNetworkStatus::Online
    }
    #[must_use]
    pub fn is_offline(&self) -> bool {
        self == &OsNetworkStatus::Offline
    }
}

#[derive(Clone)]
pub struct OsNetworkStatusObserver {
    receiver: watch::Receiver<OsNetworkStatus>,
}

impl OsNetworkStatusObserver {
    fn new(receiver: watch::Receiver<OsNetworkStatus>) -> Self {
        Self { receiver }
    }

    pub async fn wait_until_online(&mut self) {
        // `wait_for()` returns `Err` if the channel's tx has died - this
        // shouldn't be the case here, because the channel is allowed to die
        // only after the *last* instance of status watcher is dropped, and we
        // know at least one instance must be alive as it's held within `self`.
        //
        // If this logic becomes violated, the worst that can happen is that
        // this function returns even if the network connection is actually
        // offline. This is alright, because listening on network status is
        // advisory anyway - the caller is supposed to handle potential network
        // problems on their side one way or another.
        let _ = self.receiver.wait_for(OsNetworkStatus::is_online).await;
    }

    #[must_use]
    pub fn is_online(&self) -> bool {
        self.receiver.borrow().is_online()
    }

    #[must_use]
    pub fn status(&self) -> OsNetworkStatus {
        *self.receiver.borrow()
    }

    pub async fn wait_for_change(&mut self) -> OsNetworkStatus {
        // `changed()` returns `Err` if the channel's tx has died - this
        // shouldn't be the case here, because the channel is allowed to die
        // only after the *last* instance of status watcher is dropped, and we
        // know at least one instance must be alive as it's held within `self`.
        //
        // If this logic becomes violated, the worst that can happen is that
        // this function returns even if the network connection is actually
        // offline. This is alright, because listening on network status is
        // advisory anyway - the caller is supposed to handle potential network
        // problems on their side one way or another.
        let _ = self.receiver.changed().await;
        *self.receiver.borrow()
    }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait OnlineTester: Send + Sync + 'static {
    async fn check(&self, timeout: Duration) -> RequestNetworkStatus;
}

#[inline]
fn update_watcher_value<T: Copy + Eq>(channel: &watch::Sender<T>, value: T) {
    channel.send_if_modified(|old_value| {
        if *old_value == value {
            false
        } else {
            *old_value = value;
            true
        }
    });
}
