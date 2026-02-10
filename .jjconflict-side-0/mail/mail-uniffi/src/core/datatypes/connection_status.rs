use crate::UniffiEnum;
use proton_core_api::connection_status::ConnectionStatus as RealConnectionStatus;

#[derive(Debug, Clone, Copy, UniffiEnum, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// The application is online,
    Online,
    /// The application is offline,
    Offline,
    /// The application is online but the server is unreachable.
    ServerUnreachable,
}

impl From<RealConnectionStatus> for ConnectionStatus {
    fn from(status: RealConnectionStatus) -> Self {
        match status {
            RealConnectionStatus::Online => ConnectionStatus::Online,
            RealConnectionStatus::Offline => ConnectionStatus::Offline,
            RealConnectionStatus::ServerUnreachable => ConnectionStatus::ServerUnreachable,
        }
    }
}
