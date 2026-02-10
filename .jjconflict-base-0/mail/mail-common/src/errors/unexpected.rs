use proton_account_api::login::SaltError;
use proton_action_queue::action::FactoryError;
use proton_action_queue::queue::{Error as QueueError, QueuedError};
use proton_core_api::store::StoreError;
use proton_core_common::KeyHandlingError;
use proton_core_common::os::KeyChainError;
use proton_crypto_inbox::attachment::AttachmentDecryptionError;
use proton_crypto_inbox::message::MessageError;
use proton_sqlite3::MigratorError;
use stash::stash::StashError;
use std::io::{Error as IOError, ErrorKind};
use tokio::task::JoinError;

/// Categories for Unexpected error
#[derive(Debug)]
pub enum Unexpected {
    /// Error related to API values (not API error)
    Api,
    /// Error related to cryptography
    Crypto,
    /// Error related to internal app configuration
    Config,
    /// Error related to the database
    Database,
    /// Error related to draft
    Draft,
    /// Error related to an operation on file system
    FileSystem,
    /// Error related to an internal operation
    Internal,
    /// Some argument is invalid
    InvalidArgument,
    /// Error related with memory
    Memory,
    /// Error related with network
    Network,
    /// Error related to an OS operation
    Os,
    /// Error related to the event queue
    Queue,
    /// Error with no identified operation
    Unknown,
}

impl From<IOError> for Unexpected {
    #[allow(clippy::wildcard_in_or_patterns)]
    fn from(error: IOError) -> Self {
        match error.kind() {
            // Most likely to be a file system operation
            ErrorKind::NotFound
            | ErrorKind::PermissionDenied
            | ErrorKind::BrokenPipe
            | ErrorKind::AlreadyExists
            | ErrorKind::WouldBlock
            | ErrorKind::UnexpectedEof => Self::FileSystem,

            // Most likely to be a network operation
            ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::AddrInUse
            | ErrorKind::AddrNotAvailable => Self::Network,

            // Could be anything but most likely local
            ErrorKind::InvalidInput | ErrorKind::InvalidData => Self::Internal,

            ErrorKind::OutOfMemory => Self::Memory,

            // Could be any operation
            ErrorKind::TimedOut
            | ErrorKind::WriteZero
            | ErrorKind::Interrupted
            | ErrorKind::Unsupported
            | ErrorKind::Other
            | _ => Self::Unknown,
            // io_error_more Variants are unstable
            // ErrorKind::HostUnreachable => {}
            // ErrorKind::NetworkUnreachable => {}
            // ErrorKind::NetworkDown => {}
            // ErrorKind::NotADirectory => {}
            // ErrorKind::IsADirectory => {}
            // ErrorKind::DirectoryNotEmpty => {}
            // ErrorKind::ReadOnlyFilesystem => {}
            // ErrorKind::FilesystemLoop => {}
            // ErrorKind::StaleNetworkFileHandle => {}
            // ErrorKind::StorageFull => {}
            // ErrorKind::NotSeekable => {}
            // ErrorKind::FilesystemQuotaExceeded => {}
            // ErrorKind::FileTooLarge => {}
            // ErrorKind::ResourceBusy => {}
            // ErrorKind::ExecutableFileBusy => {}
            // ErrorKind::Deadlock => {}
            // ErrorKind::CrossesDevices => {}
            // ErrorKind::TooManyLinks => {}
            // ErrorKind::InvalidFilename => {}
            // ErrorKind::ArgumentListTooLong => {}
            // ErrorKind::Uncategorized => {}
        }
    }
}

impl From<StashError> for Unexpected {
    fn from(_value: StashError) -> Self {
        Self::Database
    }
}

impl From<QueueError> for Unexpected {
    fn from(_error: QueueError) -> Self {
        Self::Queue
    }
}

impl From<QueuedError> for Unexpected {
    fn from(error: QueuedError) -> Self {
        match error {
            // TODO: Check with Leander if there is a better Category
            QueuedError::Factory(_id, factory_error) => Self::from(factory_error),
            QueuedError::Action(_, _) => Self::Internal,
            QueuedError::DB(stash_error) => Self::from(stash_error),
            // TODO: Check with Leander if there is a better Category
            QueuedError::ActionNotFound(_id) => Self::Internal,
            // TODO: Check with Leander if there is a better Category
            QueuedError::ActionInExecution(_) => Self::Internal,
        }
    }
}

impl From<MigratorError> for Unexpected {
    fn from(_error: MigratorError) -> Self {
        Self::Database
    }
}

impl From<FactoryError> for Unexpected {
    fn from(_error: FactoryError) -> Self {
        Self::Internal
    }
}

impl From<KeyChainError> for Unexpected {
    fn from(_error: KeyChainError) -> Self {
        Self::Crypto
    }
}

impl From<KeyHandlingError> for Unexpected {
    fn from(_error: KeyHandlingError) -> Self {
        Self::Crypto
    }
}

impl From<AttachmentDecryptionError> for Unexpected {
    fn from(_error: AttachmentDecryptionError) -> Self {
        Self::Crypto
    }
}

impl From<MessageError> for Unexpected {
    fn from(_error: MessageError) -> Self {
        Self::Crypto
    }
}

impl From<JoinError> for Unexpected {
    fn from(_error: JoinError) -> Self {
        Self::Internal
    }
}

impl From<SaltError> for Unexpected {
    fn from(_error: SaltError) -> Self {
        Self::Crypto
    }
}

impl From<StoreError> for Unexpected {
    fn from(_error: StoreError) -> Self {
        Self::Crypto
    }
}
