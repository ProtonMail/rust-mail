use crate::models::LabelError;
use proton_action_queue::action::WriterGuardError;
use proton_action_queue::network::{
    DummyWaitForOnlineSubscribtion, WaitForOnline, WaitForOnlineSubscribtion,
};
use proton_api_core::connection_status::ConnectionStatus;
use proton_api_core::service::ApiServiceError;
use proton_api_core::status_watcher::{StatusWatcher, StatusWatcherSubscriber};
use stash::stash::StashError;
use tokio::sync::watch::Receiver;

#[derive(Debug, thiserror::Error)]
pub enum CoreActionError {
    #[error("Http: {0}")]
    Http(#[from] ApiServiceError),
    #[error("Stash: {0}")]
    Stash(#[from] StashError),
    #[error("Label: {0}")]
    Label(#[from] LabelError),
    #[error("No input provided")]
    NoInput,
    #[error("Queue Writer Guard Expired")]
    QueueWriterGuardExpired,
    #[error("Other: {0}")]
    Other(anyhow::Error),
}

impl proton_action_queue::action::Error for CoreActionError {
    fn is_network_failure(&self) -> bool {
        if let Self::Http(e) = self {
            e.is_network_failure()
        } else {
            false
        }
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, Self::QueueWriterGuardExpired)
    }
}

impl From<WriterGuardError> for CoreActionError {
    fn from(value: WriterGuardError) -> Self {
        match value {
            WriterGuardError::Expired => Self::QueueWriterGuardExpired,
            WriterGuardError::Stash(e) => Self::Stash(e),
        }
    }
}

pub trait WaitForOnlineSubscribtionExt: WaitForOnlineSubscribtion {
    fn create(watcher: StatusWatcher) -> Self;
}

impl WaitForOnlineSubscribtionExt for DummyWaitForOnlineSubscribtion {
    fn create(_: StatusWatcher) -> Self {
        Self
    }
}

/// Creates an imlementation of [`WaitForOnline`] trait that uses
/// API Status Watcher
///
pub struct CheckNetworkStatusSubscriber {
    watcher: StatusWatcher,
}

impl WaitForOnlineSubscribtionExt for CheckNetworkStatusSubscriber {
    fn create(watcher: StatusWatcher) -> Self {
        Self { watcher }
    }
}

impl WaitForOnlineSubscribtion for CheckNetworkStatusSubscriber {
    fn subscribe(&self) -> impl WaitForOnline {
        CheckForNetworkStatus {
            receiver: self.watcher.subscribe(),
        }
    }
}

/// An implementation of [`WaitForOnline`] trai that uses API Status Watcher
///
pub struct CheckForNetworkStatus {
    receiver: Receiver<ConnectionStatus>,
}

#[async_trait::async_trait]
impl WaitForOnline for CheckForNetworkStatus {
    async fn wait_for_online(&mut self) {
        self.receiver.wait_for_online().await;
    }
}
