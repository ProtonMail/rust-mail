#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// The application is online,
    Online,
    /// The application is offline,
    Offline,
    /// The application is online but the server is unreachable.
    ServerUnreachable,
}
