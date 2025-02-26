use proton_mail_common::errors::api_service_error::UserApiServiceError as RealUserApiServiceError;
use tracing::error;

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
    TooManyRequest(String),

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
}

impl From<RealUserApiServiceError> for UserApiServiceError {
    fn from(value: RealUserApiServiceError) -> Self {
        error!("UserApiServiceError from {value:?}");
        match value {
            RealUserApiServiceError::BadRequest(text) => Self::BadRequest(text),
            RealUserApiServiceError::Unauthorized(text) => Self::Unauthorized(text),
            RealUserApiServiceError::NotFound(text) => Self::NotFound(text),
            RealUserApiServiceError::UnprocessableEntity(text) => Self::UnprocessableEntity(text),
            RealUserApiServiceError::TooManyRequests(text) => Self::TooManyRequest(text),
            RealUserApiServiceError::InternalServerError(text) => Self::InternalServerError(text),
            RealUserApiServiceError::NotImplemented(text) => Self::NotImplemented(text),
            RealUserApiServiceError::BadGateway(text) => Self::BadGateway(text),
            RealUserApiServiceError::ServiceUnavailable(text) => Self::ServiceUnavailable(text),
            RealUserApiServiceError::OtherHttpError(code, text) => Self::OtherHttpError(code, text),
        }
    }
}
