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
    pub fn is_online(&self) -> bool {
        matches!(self, ConnectionStatus::Online)
    }
    pub fn is_offline(&self) -> bool {
        !self.is_online()
    }
}
