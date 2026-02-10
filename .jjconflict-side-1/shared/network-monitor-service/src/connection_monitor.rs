use crate::{
    NetworkMonitorService, NetworkStatusObserver, RequestNetworkStatus, update_watcher_value,
};
use tokio::sync::watch;

#[derive(Debug, Clone)]
enum ConnectionMonitorMode {
    Standalone,
    Monitored(watch::Sender<RequestNetworkStatus>),
}

#[derive(Clone)]
pub struct ConnectionMonitor {
    request_network_status: watch::Sender<RequestNetworkStatus>,
    mode: ConnectionMonitorMode,
}

impl ConnectionMonitor {
    /// In standalone mode the connection monitor is not registered with a network monitor
    /// service and only monitors the network connection it is attached to. This
    /// is mostly a compatability method
    #[must_use]
    pub fn standalone() -> Self {
        let (sender, _) = watch::channel(RequestNetworkStatus::Online);
        Self {
            request_network_status: sender,
            mode: ConnectionMonitorMode::Standalone,
        }
    }

    #[must_use]
    pub fn monitored(monitor: &NetworkMonitorService) -> Self {
        Self {
            request_network_status: monitor.request_watcher(),
            mode: ConnectionMonitorMode::Monitored(monitor.subscriber_watcher()),
        }
    }

    #[must_use]
    pub fn network_status_observer(&self) -> NetworkStatusObserver {
        let receiver = match &self.mode {
            ConnectionMonitorMode::Standalone => self.request_network_status.subscribe(),
            ConnectionMonitorMode::Monitored(sender) => sender.subscribe(),
        };
        NetworkStatusObserver::new(receiver)
    }

    pub fn update_request_status(&self, status: RequestNetworkStatus) {
        update_watcher_value(&self.request_network_status, status);
    }
}
