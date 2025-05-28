use proton_core_api::service::ApiServiceError;

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

impl From<ApiServiceError> for UserApiServiceError {
    fn from(error: ApiServiceError) -> Self {
        use ApiServiceError::{
            AuthStore, BadGateway, BadRequest, ConnectionError, InternalServerError, NetworkError,
            NotFound, NotImplemented, OtherHttpError, ParseEndpoint, QueryStringError, Redirect,
            RequestError, ResponseError, ServiceUnavailable, Timeout, TooManyRequests,
            Unauthorized, UnknownError, UnprocessableEntity, Utf8DecodingError,
        };

        match error {
            BadRequest(_, info) => Self::BadRequest(format!("{info:?}")),
            Unauthorized(_, info) => Self::Unauthorized(format!("{info:?}")),
            NotFound(_, info) => Self::NotFound(format!("{info:?}")),
            UnprocessableEntity(_, info) => Self::UnprocessableEntity(format!("{info:?}")),
            TooManyRequests(_, info) => Self::TooManyRequests(format!("{info:?}")),
            InternalServerError(_, info) => Self::InternalServerError(format!("{info:?}")),
            NotImplemented(_, info) => Self::NotImplemented(format!("{info:?}")),
            BadGateway(_, info) => Self::BadGateway(format!("{info:?}")),
            ServiceUnavailable(_, info) => Self::ServiceUnavailable(format!("{info:?}")),

            OtherHttpError(code, _, info) => {
                Self::OtherHttpError(code.as_u16(), format!("{info:?}"))
            }

            ConnectionError(_) | NetworkError(_) | Redirect(_, _) | Timeout(_) => {
                Self::OtherNetwork(error.to_string())
            }

            QueryStringError(_) | RequestError(_) | ResponseError(_) | Utf8DecodingError(_)
            | ParseEndpoint(_) | AuthStore(_) | UnknownError(_) => {
                Self::Internal(error.to_string())
            }
        }
    }
}
