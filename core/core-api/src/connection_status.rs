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
