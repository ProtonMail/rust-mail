use crate::service::ApiServiceError;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiServiceObservabilityResponse {
    Success,
    Http4xx,
    Http5xx,

    /// An internal muon error has occurred. This could be due to a network
    /// error, or a misconfiguration, causing the request to fail.
    NetworkError,

    /// There has been a failure in compositing the HTTP request/query parameters to send or
    /// the response received
    SerializationError,

    /// An unknown error has occurred. These should be monitored and specific
    /// handling added in.
    Unknown,
}

impl From<Option<&ApiServiceError>> for ApiServiceObservabilityResponse {
    fn from(value: Option<&ApiServiceError>) -> Self {
        match value {
            None => ApiServiceObservabilityResponse::Success,
            Some(err) => err.into(),
        }
    }
}

impl From<&ApiServiceError> for ApiServiceObservabilityResponse {
    fn from(value: &ApiServiceError) -> Self {
        match value {
            ApiServiceError::Timeout(..)
            | ApiServiceError::BadRequest(..)
            | ApiServiceError::Unauthorized(..)
            | ApiServiceError::Forbidden(..)
            | ApiServiceError::NotFound(..)
            | ApiServiceError::UnprocessableEntity(..)
            | ApiServiceError::TooManyRequests(..) => ApiServiceObservabilityResponse::Http4xx,

            ApiServiceError::InternalServerError(..)
            | ApiServiceError::NotImplemented(..)
            | ApiServiceError::ServiceUnavailable(..)
            | ApiServiceError::BadGateway(..) => ApiServiceObservabilityResponse::Http5xx,

            ApiServiceError::OtherHttpError(..)
            | ApiServiceError::UnknownError(..)
            | ApiServiceError::AuthStore(..)
            | ApiServiceError::Redirect(_, _) => ApiServiceObservabilityResponse::Unknown,

            ApiServiceError::ConnectionError(..) | ApiServiceError::NetworkError(..) => {
                ApiServiceObservabilityResponse::NetworkError
            }

            ApiServiceError::Utf8DecodingError(..)
            | ApiServiceError::QueryStringError(..)
            | ApiServiceError::ParseEndpoint(..)
            | ApiServiceError::RequestError(..)
            | ApiServiceError::ResponseError(..) => {
                ApiServiceObservabilityResponse::SerializationError
            }
        }
    }
}
