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
    /// note: Remember to replace observer for tests, they will interfere with each other otherwise
    ///
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = watch::channel(ConnectionStatus::Online);
        Self {
            subscribers: Arc::new(sender),
            observer: StatusObserver::new(),
        }
    }

    /// Replace the default observer, useful when running tests.
    ///
    #[must_use]
    pub fn with_observer(self, observer: StatusObserver) -> Self {
        Self { observer, ..self }
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
                        interval.reset();
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
        let new_status = observer.status(api.clone()).await;
        let current = subscribers.borrow();

        if *current != new_status {
            subscribers.send_replace(*current);
        }
    }
}
