use crate::errors::unexpected::Unexpected;
use proton_core_api::service::ApiServiceError;

#[derive(Debug)]
pub enum UserApiServiceError {
    /// 400: The request has been made incorrectly.
    BadRequest(String),

    /// 401: The request was rejected due to authentication failure.
    Unauthorized(String),

    /// 403: The request was refused due to insufficient permissions.
    Forbidden(String),

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
}

impl TryFrom<ApiServiceError> for UserApiServiceError {
    type Error = Unexpected;

    fn try_from(error: ApiServiceError) -> Result<Self, Self::Error> {
        use ApiServiceError::*;

        match error {
            BadRequest(_, info) => Ok(Self::BadRequest(format!("{info:?}"))),
            Unauthorized(_, info) => Ok(Self::Unauthorized(format!("{info:?}"))),
            Forbidden(_, info) => Ok(Self::Forbidden(format!("{info:?}"))),
            NotFound(_, info) => Ok(Self::NotFound(format!("{info:?}"))),
            UnprocessableEntity(_, info) => Ok(Self::UnprocessableEntity(format!("{info:?}"))),
            TooManyRequests(_, info) => Ok(Self::TooManyRequests(format!("{info:?}"))),
            InternalServerError(_, info) => Ok(Self::InternalServerError(format!("{info:?}"))),
            NotImplemented(_, info) => Ok(Self::NotImplemented(format!("{info:?}"))),
            BadGateway(_, info) => Ok(Self::BadGateway(format!("{info:?}"))),
            ServiceUnavailable(_, info) => Ok(Self::ServiceUnavailable(format!("{info:?}"))),

            OtherHttpError(code, _, info) => {
                Ok(Self::OtherHttpError(code.as_u16(), format!("{info:?}")))
            }

            ConnectionError(_) | NetworkError(_) | Redirect(_, _) | Timeout(_) => {
                Err(Unexpected::Network)
            }

            QueryStringError(_) | RequestError(_) | ResponseError(_) | Utf8DecodingError(_)
            | ParseEndpoint(_) | AuthStore(_) | UnknownError(_) => Err(Unexpected::Internal),
        }
    }
}
