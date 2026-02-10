use proton_mail_common::Unexpected as RealUnexpected;

/// Categories for Unexpected error
#[derive(Debug, uniffi::Enum)]
pub enum UnexpectedError {
    /// Error related to API values (not API error)
    Api,
    /// Error related to cryptography
    Crypto,
    /// Error related to internal app configuration
    Config,
    /// Error related to the database
    Database,
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
    /// Error related to the composing draft
    Draft,
    /// Error mapping failed, this is serious issue and has to be addressed asap
    ErrorMapping,
    /// Error with no identified operation
    Unknown,
}

impl From<RealUnexpected> for UnexpectedError {
    fn from(value: RealUnexpected) -> Self {
        match value {
            RealUnexpected::Api => Self::Api,
            RealUnexpected::Crypto => Self::Crypto,
            RealUnexpected::Config => Self::Config,
            RealUnexpected::Database => Self::Database,
            RealUnexpected::FileSystem => Self::FileSystem,
            RealUnexpected::Internal => Self::Internal,
            RealUnexpected::InvalidArgument => Self::InvalidArgument,
            RealUnexpected::Memory => Self::Memory,
            RealUnexpected::Network => Self::Network,
            RealUnexpected::Os => Self::Os,
            RealUnexpected::Queue => Self::Queue,
            RealUnexpected::Draft => Self::Draft,
            RealUnexpected::Unknown => Self::Unknown,
        }
    }
}
