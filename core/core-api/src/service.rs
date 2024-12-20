#![allow(clippy::module_name_repetitions)]

use crate::services::proton::prelude::*;
use crate::store::StoreError;
use muon::{Method, Status};
use serde_json::Error as JsonError;
use serde_qs::Error as QueryStringError;
use std::fmt::{Debug, Display};
use std::string::FromUtf8Error;
use thiserror::Error;
use tracing::error;

/// A result containing an error that defaults to `ApiServiceError`.
pub type ApiServiceResult<T, E = ApiServiceError> = Result<T, E>;

/// The possible errors that can occur when using an external API.
///
/// The possible errors fall into a few categories:
///
///   - Network level
///   - Protocol level
///   - Data level
///   - Logic level
///
/// ## Network level
///
/// If there are problems establishing a network connection to the external API,
/// or issues during the exchange of data, then these errors will be generated.
/// These are Reqwest errors, and are at a low level of interaction, and not at
/// a protocol level.
///
/// These errors are reported internally by this system, from the Reqwest
/// library.
///
/// ## Protocol level
///
/// HTTP errors will be relayed through as they are, and will not represent
/// anything other than there being a problem at a protocol level, with the
/// exchange of request and response. These errors could be caused by incorrect
/// data being sent to the external API, or by a problem with the external API.
/// Problems reported by the external API could be due to this system doing
/// something wrong, but equally could be due to the external API service doing
/// something wrong.
///
/// The errors that can occur here are related to bad requests, missing
/// resources, invalid data, and internal server errors — all from the
/// perspective of the external API service. These are all errors that are
/// reported back by the external API.
///
/// ## Data level
///
/// Errors of this nature are related to the data that is returned from the
/// external API. These errors are caused by the external API returning data
/// that is not in the expected format, or is missing required fields. For
/// whatever reason, deserialisation of the data has failed — so the response
/// was potentially received correctly, and was complete, but something about
/// the data meant it could not be processed.
///
/// Data errors refer here generally to all JSON deserialisation errors, which
/// are first and foremost schema validation errors, but could also be flagged
/// at an intra-field level. Structure, type, and format errors are all errors
/// of this nature.
///
/// ## Logic level
///
/// These errors are related to the logic of the system, and are not related to
/// the external API. These errors are caused by the system not being able to
/// carry out an expected task or function, despite having valid data.
///
#[derive(Debug, Error)]
pub enum ApiServiceError {
    //  NETWORK ERRORS
    //==========================================================================
    /// An internal muon error has occurred, specifically when attempting to make a connection.
    #[error("Network connection error: {0}")]
    ConnectionError(String),

    /// An internal muon error has occurred. This could be due to a network
    /// error, or a misconfiguration, causing the request to fail.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// An internal muon error has occurred, specifically, we have been redirected.
    #[error("Redirect error for {0}: {1}")]
    Redirect(String, String),

    /// An internal muon error has occurred, specifically, the HTTP request has timed out.
    #[error("Timeout: {0}")]
    Timeout(String),

    //  PROTOCOL ERRORS
    //==========================================================================
    /// 400: The request has been made incorrectly.
    #[error("Bad request: {0}. {1}")]
    BadRequest(String, String),

    /// 401: The request was rejected due to authentication failure.
    #[error("Unauthorized: {0}. {1}")]
    Unauthorized(String, String),

    /// 404: The URL requested on the external API was not found.
    #[error("Not found: {0}. {1}")]
    NotFound(String, String),

    /// 422: The data/request provided was invalid in terms or structure or
    /// contents, and could not be processed by the external API service.
    #[error("Unprocessable entity: {0}. {1}")]
    UnprocessableEntity(String, String),

    /// 429: The client made too many requests to the server.
    #[error("Too many requests: {0}. {1}")]
    TooManyRequest(String, String),

    /// 500: Something is wrong with the external API service.
    #[error("Internal server error: {0}. {1}")]
    InternalServerError(String, String),

    /// 501: The server either does not recognize the request method, or it lacks the ability to
    /// fulfil the request.
    #[error("Not Implemented: {0}. {1}")]
    NotImplemented(String, String),

    /// 502: The server was acting as a gateway or proxy and received an invalid response from the
    /// upstream server.
    #[error("Bad gateway: {0}. {1}")]
    BadGateway(String, String),

    /// 503: The server cannot handle the request (because it is overloaded or down for maintenance).
    #[error("Service Unavailable: {0}. {1}")]
    ServiceUnavailable(String, String),

    /// Any other HTTP error which is not currently handled.
    #[error("HTTP error {0}: {1}. {2}")]
    OtherHttpError(Status, String, String),

    //  DATA ERRORS
    //==========================================================================
    /// There has been a failure in en/decoding the JSON data sent/received to/from the
    /// external API into/from the appropriate structs.
    #[error("JSON (de)serialization error: {0}, context: {1}")]
    JsonError(JsonError, String),

    /// There has been a failure in encoding the query parameters to be sent with
    /// an outgoing HTTP request.
    #[error("Query encoding error: {0}")]
    QueryStringError(#[from] QueryStringError),

    /// There has been a failure in compositing the HTTP request to send. Note
    /// that this is not a network error, but an error in the request itself.
    #[error("Request composition error: {0}")]
    RequestError(String),

    /// There has been a failure in parsing the HTTP response received. Note
    /// that this is not a network error, but an error in the response itself.
    #[error("Response parsing error: {0}")]
    ResponseError(String),

    /// There has been a failure in decoding the data returned from the external
    /// API into valid UTF8 text.
    #[error("UTF8 decoding error: {0}")]
    Utf8DecodingError(FromUtf8Error),

    //  LOGIC ERRORS
    //==========================================================================
    /// An error has been reported by the implementing service. We don't worry
    /// too much about use of `Box` or dynamic traits here, as performance is
    /// not critical in this context.
    #[error("API Service error: {0}")]
    ServiceError(Box<dyn ServiceError>),

    /// An unsupported HTTP method was specified.
    #[error("Unsupported HTTP method: {0}")]
    UnsupportedHttpMethod(Method),

    /// Authentication store operation failed.
    #[error("Authentication Store error: {0}")]
    AuthStore(#[from] StoreError),

    /// An unknown error has occurred. These should be monitored and specific
    /// handling added in.
    #[error("Unknown error: {0}")]
    UnknownError(String),
}

impl ApiServiceError {
    /// Check if the error is the result of a network failure.
    ///
    /// An error is considered a network failure the server replies with 429/5xx HTTP status codes
    /// or there was an issue with the underlying network transport layer.
    #[must_use]
    pub fn is_network_failure(&self) -> bool {
        match self {
            ApiServiceError::Redirect(_, _)
            | ApiServiceError::Timeout(_)
            | ApiServiceError::NetworkError(_)
            | ApiServiceError::ConnectionError(_)
            | ApiServiceError::TooManyRequest(_, _)
            | ApiServiceError::BadGateway(_, _)
            | ApiServiceError::NotImplemented(_, _)
            | ApiServiceError::ServiceUnavailable(_, _)
            | ApiServiceError::InternalServerError(_, _) => true,
            ApiServiceError::OtherHttpError(code, _, _) => code.as_u16() >= 500,
            _ => false,
        }
    }
}

impl ApiServiceError {
    /// Attempts to extract the Proton error from the API error.
    ///
    /// Returns `None` if the error is not present or
    /// failed to deserialize.
    pub fn to_proton_error(&self) -> Option<ApiErrorInfo> {
        //TODO(ET-1700): This should be returned by default.
        let (ApiServiceError::BadRequest(_, body)
        | ApiServiceError::Unauthorized(_, body)
        | ApiServiceError::NotFound(_, body)
        | ApiServiceError::UnprocessableEntity(_, body)
        | ApiServiceError::TooManyRequest(_, body)
        | ApiServiceError::ServiceUnavailable(_, body)
        | ApiServiceError::OtherHttpError(_, _, body)) = self
        else {
            return None;
        };

        match ApiErrorInfo::from_json(body) {
            Ok(e) => Some(e),
            Err(e) => {
                error!("Failed to parse API error: {}", e);
                None
            }
        }
    }
}

/// Marker trait for service errors.
pub trait ServiceError: Debug + Display + Send + Sync {}
