use crate::status_observer::StatusObserver;
use crate::{connection_status::ConnectionStatus, services::proton::Proton};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;
use tokio::{sync::watch, time};
use tokio_util::task::AbortOnDropHandle;

/// A `StatusWatcher` keeps track of the connection status and provides an interface to observe the changes.
///
/// It will watch `StatusObserver` updates and periodically request current status.
///
#[derive(Debug, Clone)]
pub struct StatusWatcher {
    status_tx: watch::Sender<ConnectionStatus>,
    online_tx: watch::Sender<bool>,
    observer: StatusObserver,
    task: Option<Arc<AbortOnDropHandle<()>>>,
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
    #[must_use]
    pub fn new() -> Self {
        Self::with_observer(StatusObserver::new())
    }

    /// Construct new `StatusWatcher` with custom observer, useful when running tests.
    #[must_use]
    pub fn with_observer(observer: StatusObserver) -> Self {
        let (status_tx, _) = watch::channel(ConnectionStatus::Online);
        let (online_tx, _) = watch::channel(true);

        Self {
            observer,
            status_tx,
            online_tx,
            task: None,
        }
    }

    /// Clone underlying observer
    pub fn observer(&self) -> StatusObserver {
        self.observer.clone()
    }

    /// Returns a channel that observes the network connection status.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<ConnectionStatus> {
        self.status_tx.subscribe()
    }

    /// Returns a channel that observes the network connection status, but
    /// simplified only to the "are we online" state.
    #[must_use]
    pub fn subscribe_to_online(&self) -> watch::Receiver<bool> {
        self.online_tx.subscribe()
    }

    /// Initialize background task for notifying subscribers
    pub fn initialize(&mut self, api: Proton) {
        let status_tx = self.status_tx.clone();
        let online_tx = self.online_tx.clone();
        let observer = self.observer.clone();

        let task = tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(10));
            let mut on_update = observer.subscribe();

            // Make first call lazy and wait for real data.
            on_update.mark_unchanged();

            loop {
                tokio::select! {
                    _ = interval.tick() => {}
                    Ok(()) = on_update.changed() => {}
                }

                Self::update_status(&status_tx, &online_tx, &observer, &api).await;
            }
        });

        self.task = Some(Arc::new(AbortOnDropHandle::new(task)));
    }

    async fn update_status(
        status_tx: &watch::Sender<ConnectionStatus>,
        online_tx: &watch::Sender<bool>,
        observer: &StatusObserver,
        api: &Proton,
    ) {
        let old = *status_tx.borrow();
        let new = observer.status(api.clone()).await;

        if new == old {
            // Avoid calling `.send_replace()`, so that we don't wake up the
            // subscribers just to tell them "ha, nothing changed, just a prank"
        } else {
            status_tx.send_replace(new);
            online_tx.send_replace(new.is_online());
        }
    }
}
