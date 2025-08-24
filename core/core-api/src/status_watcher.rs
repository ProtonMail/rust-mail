use crate::status_observer::StatusObserver;
use crate::{connection_status::ConnectionStatus, services::proton::Proton};
use proton_task_service::SpawnerRef;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;
use tokio::{sync::watch, time};
use tokio_util::task::AbortOnDropHandle;

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

impl StatusWatcher {
    #[must_use]
    pub fn new(spawner: SpawnerRef) -> Self {
        Self::with_observer(StatusObserver::new(spawner))
    }

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

    pub fn observer(&self) -> StatusObserver {
        self.observer.clone()
    }

    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<ConnectionStatus> {
        self.status_tx.subscribe()
    }

    #[must_use]
    pub fn subscribe_to_online(&self) -> watch::Receiver<bool> {
        self.online_tx.subscribe()
    }

    pub fn initialize(&mut self, api: Proton) {
        let status_tx = self.status_tx.clone();
        let online_tx = self.online_tx.clone();
        let observer = self.observer.clone();

        if let Some(task) = self.task.as_ref() {
            if !task.is_finished() {
                // We assume at that point that the status watcher is already
                // initialized
                return;
            }
        }

        let task = tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(10));
            let mut on_update = observer.subscribe();

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
        let new = observer.status(api.clone()).await;

        // Avoid calling `.send_replace()`, so that we don't wake up the
        // subscribers just to tell them "ha, nothing changed, just a prank"
        status_tx.send_if_modified(|old| {
            if new == *old {
                false
            } else {
                *old = new;
                true
            }
        });

        online_tx.send_if_modified(|old| {
            let new = new.is_online();

            if new == *old {
                false
            } else {
                *old = new;
                true
            }
        });
    }
}
