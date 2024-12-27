use crate::errors::unexpected::Unexpected;
use proton_api_core::service::ApiServiceError;

#[derive(Debug)]
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

impl TryFrom<ApiServiceError> for UserApiServiceError {
    type Error = Unexpected;

    fn try_from(error: ApiServiceError) -> Result<Self, Self::Error> {
        match error {
            ApiServiceError::BadRequest(_, text) => Ok(Self::BadRequest(text)),
            ApiServiceError::Unauthorized(_, text) => Ok(Self::Unauthorized(text)),
            ApiServiceError::NotFound(_, text) => Ok(Self::NotFound(text)),
            ApiServiceError::UnprocessableEntity(_, text) => Ok(Self::UnprocessableEntity(text)),
            ApiServiceError::TooManyRequest(_, text) => Ok(Self::TooManyRequest(text)),
            ApiServiceError::InternalServerError(_, text) => Ok(Self::InternalServerError(text)),
            ApiServiceError::NotImplemented(_, text) => Ok(Self::NotImplemented(text)),
            ApiServiceError::BadGateway(_, text) => Ok(Self::BadGateway(text)),
            ApiServiceError::ServiceUnavailable(_, text) => Ok(Self::ServiceUnavailable(text)),
            ApiServiceError::OtherHttpError(code, _, text) => {
                Ok(Self::OtherHttpError(code.as_u16(), text))
            }

            ApiServiceError::ConnectionError(_)
            | ApiServiceError::NetworkError(_)
            | ApiServiceError::Redirect(_, _)
            | ApiServiceError::Timeout(_) => Err(Unexpected::Network),

            ApiServiceError::QueryStringError(_)
            | ApiServiceError::RequestError(_)
            | ApiServiceError::ResponseError(_)
            | ApiServiceError::Utf8DecodingError(_)
            | ApiServiceError::AuthStore(_)
            | ApiServiceError::UnknownError(_) => Err(Unexpected::Internal),
        }
    }
}
