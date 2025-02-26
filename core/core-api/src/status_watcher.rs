use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::{sync::Arc, time::Duration};

use crate::status_observer::StatusObserver;
use crate::{connection_status::ConnectionStatus, services::proton::Proton};
use tokio::{sync::watch, time};

/// A `StatusWatcher` keeps track of the connection status and provides an interface to observe the changes.
///
/// It will watch `StatusObserver` updates and periodically request current status.
///
#[derive(Debug, Clone)]
pub struct StatusWatcher {
    subscribers: Arc<watch::Sender<ConnectionStatus>>,
    observer: StatusObserver,
}

impl Deref for StatusWatcher {
    type Target = StatusObserver;

    fn deref(&self) -> &Self::Target {
        &self.observer
    }
}

impl DerefMut for StatusWatcher {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.observer
    }
}

impl Default for StatusWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusWatcher {
    /// Construct new `StatusWatcher` with default shared state Observer.
    ///
    #[must_use]
    pub fn new() -> Self {
        Self::with_observer(StatusObserver::new())
    }

    /// Construct new `StatusWatcher` with custom observer, useful when running tests.
    ///
    #[must_use]
    pub fn with_observer(observer: StatusObserver) -> Self {
        let (sender, _) = watch::channel(ConnectionStatus::Online);
        Self {
            observer,
            subscribers: Arc::new(sender),
        }
    }

    /// Clone underlying observer
    pub fn observer(&self) -> StatusObserver {
        self.observer.clone()
    }

    /// Subscribe for notifications of updates to the status
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<ConnectionStatus> {
        self.subscribers.subscribe()
    }

    /// Initialize background task for notifying subscribers
    pub fn initialize(&self, api: Proton) {
        let subscribers = Arc::downgrade(&self.subscribers);
        let observer = self.observer.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(10));
            let mut on_update = observer.on_updates();
            // Make first call lazy and wait for real data.
            on_update.mark_unchanged();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let Some(subscribers) = subscribers.upgrade() else {
                            return;
                        };
                        Self::update_status(&subscribers, &observer, &api).await;
                    }
                    Ok(()) = on_update.changed() => {
                        let Some(subscribers) = subscribers.upgrade() else {
                            return;
                        };
                        Self::update_status(&subscribers, &observer, &api).await;
                    }
                }
            }
        });
    }

    /// Update subscribers on status change
    async fn update_status(
        subscribers: &watch::Sender<ConnectionStatus>,
        observer: &StatusObserver,
        api: &Proton,
    ) {
        let current: ConnectionStatus = *subscribers.borrow();
        let new_status = observer.status(api.clone()).await;

        if current != new_status {
            subscribers.send_replace(new_status);
        }
    }
}

pub trait StatusWatcherSubscriber {
    fn wait_for_online(&mut self) -> impl Future<Output = ()> + Send;
}

impl StatusWatcherSubscriber for watch::Receiver<ConnectionStatus> {
    async fn wait_for_online(&mut self) {
        while self.changed().await.is_ok() {
            // first call to `.changed()` returns immediately
            if self.borrow().is_online() {
                break;
            }
        }
    }
}
