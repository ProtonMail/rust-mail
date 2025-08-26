use proton_network_monitor_service::RequestNetworkStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// The application is online,
    Online,
    /// The application is offline,
    Offline,
    /// The application is online but the server is unreachable.
    ServerUnreachable,
}

impl ConnectionStatus {
    /// Check if the application is online and server is reachable.
    #[must_use]
    pub fn is_online(&self) -> bool {
        matches!(self, ConnectionStatus::Online)
    }

    /// Check if application is offline or server is unreachable.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        !self.is_online()
    }
}

impl From<RequestNetworkStatus> for ConnectionStatus {
    fn from(status: RequestNetworkStatus) -> Self {
        match status {
            RequestNetworkStatus::Offline => ConnectionStatus::Offline,
            RequestNetworkStatus::Online => ConnectionStatus::Online,
            RequestNetworkStatus::ServerUnreachable => ConnectionStatus::ServerUnreachable,
        }
    }
}
