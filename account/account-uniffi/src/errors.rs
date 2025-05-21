#[derive(Debug, uniffi::Enum)]
pub enum UserApiServiceError {
    /// 400: The request has been made incorrectly.
    BadRequest(String),

    /// 401: The request was rejected due to authentication failure.
    Unauthorized(String),

    /// 404: The URL requested on the external API was not found.
    NotFound(String),

    /// 422: The data/request provided was invalid in terms or structure or
    /// contents, and could not be processed by the external API service.
    UnprocessableEntity(String),

    /// 429: The client made too many requests to the server.
    TooManyRequests(String),

    /// 500: Something is wrong with the external API service.
    InternalServerError(String),

    /// 501: The server either does not recognize the request method, or it lacks the ability to
    /// fulfil the request.
    NotImplemented(String),

    /// 502: The server was acting as a gateway or proxy and received an invalid response from the
    /// upstream server.
    BadGateway(String),

    /// 503: The server cannot handle the request (because it is overloaded or down for maintenance).
    ServiceUnavailable(String),

    /// Any other HTTP error which is not currently handled.
    OtherHttpError(u16, String),

    /// Any other network error which is not currently handled.
    OtherNetwork(String),

    /// Any other error which is not currently handled.
    Internal(String),
}

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
