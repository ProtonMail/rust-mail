use std::ops::Deref;
use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use crate::status_observer::StatusObserver;
use crate::{connection_status::ConnectionStatus, services::proton::Proton};
use tokio::{
    sync::{
        watch::{self, Receiver, Sender},
        RwLock,
    },
    time,
};

#[derive(Debug, Clone)]
pub struct StatusWatcher {
    subsribers: Arc<RwLock<Sender<ConnectionStatus>>>,
    current: Arc<RwLock<ConnectionStatus>>,
    observer: StatusObserver,
}

impl Deref for StatusWatcher {
    type Target = StatusObserver;

    fn deref(&self) -> &Self::Target {
        &self.observer
    }
}

impl StatusWatcher {
    #[must_use]
    pub fn new(api: Proton) -> Self {
        let (sender, _) = watch::channel(ConnectionStatus::Online);
        let this = Self {
            subsribers: Arc::new(RwLock::new(sender)),
            current: Arc::new(RwLock::new(ConnectionStatus::Online)),
            observer: StatusObserver::new(),
        };

        this.initialize(api);
        this
    }

    #[cfg(any(test, debug_assertions))]
    #[must_use]
    pub fn test(api: Proton) -> Self {
        let (sender, _) = watch::channel(ConnectionStatus::Online);
        let this = Self {
            subsribers: Arc::new(RwLock::new(sender)),
            current: Arc::new(RwLock::new(ConnectionStatus::Online)),
            observer: StatusObserver::test(),
        };

        this.initialize(api);
        this
    }

    pub async fn subscribe(&self) -> Receiver<ConnectionStatus> {
        self.subsribers.read().await.subscribe()
    }

    fn initialize(&self, api: Proton) {
        let subscribers = Arc::downgrade(&self.subsribers);
        let observer = self.observer.clone();
        let current_handle = self.current.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(10));
            let mut on_update = observer.on_updates();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        Self::update_status(&subscribers, &observer, &api, &current_handle).await;
                    }
                    Ok(()) = on_update.changed() => {
                        Self::update_status(&subscribers, &observer, &api, &current_handle).await;
                    }
                }
            }
        });
    }

    async fn update_status(
        subscribers: &Weak<RwLock<Sender<ConnectionStatus>>>,
        observer: &StatusObserver,
        api: &Proton,
        current_handle: &Arc<RwLock<ConnectionStatus>>,
    ) {
        let Some(subscribers) = subscribers.upgrade() else {
            return;
        };
        let subscribers = subscribers.read().await;

        if subscribers.receiver_count() > 0 {
            let new_status = observer.status(api.clone()).await;
            let mut current = current_handle.write().await;

            if *current != new_status {
                *current = new_status;

                if let Err(e) = subscribers.send(*current) {
                    tracing::error!("Cant send notification on status change {e}");
                }
            }
        }
    }
}
