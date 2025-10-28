//! Common types used by the Proton API.
//!
//! This module provides child data types that are used for both requests and
//! responses, and are not specific to any one endpoint.
//!
//! The structs in this module should NOT have any business logic or other
//! functionality.
//!

use derive_more::Display;
use muon::client::middleware::AuthErr;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::fmt::Debug;
use std::time::Duration;

use crate::service::ApiServiceError;
use muon::common::Timeout;
use muon::error::ErrorKind as MuonErrorKind;
use muon::{Status, StatusErr};
use std::error::Error;

/// Defines timeout values.
pub struct Timeouts;

impl Timeouts {
    pub const QUARTER_SECOND: Duration = Duration::from_millis(250);
    pub const ONE_SECOND: Duration = Duration::from_secs(1);
    pub const TWO_SECONDS: Duration = Duration::from_secs(2);
    pub const QUARTER_MINUTE: Duration = Duration::from_secs(15);
    pub const ONE_MINUTE: Duration = Duration::from_secs(60);
}

/// Additional information about an API service error.
///
/// If a response is received with an HTTP status code that indicates a protocol
/// error, then it may be accompanied by additional information about the error.
/// This struct provides a way to access that information.
///
#[derive(Clone, Debug, Display, Default, Deserialize, Eq, PartialEq)]
#[cfg_attr(feature = "mocks", derive(serde::Serialize))]
#[display("{code}: {error:?} ({details:?})")]
#[serde(rename_all = "PascalCase")]
pub struct ApiErrorInfo {
    /// Internal API code.
    pub code: u32,

    /// Optional error message that may be present.
    pub error: Option<String>,

    /// Optional JSON type with error details.
    pub details: Option<JsonValue>,
}

impl ApiErrorInfo {
    /// Parse the error from json data.
    ///
    /// # Errors
    ///
    /// Returns error if the format is not valid or expected json.
    pub fn from_json(json: impl AsRef<str>) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json.as_ref())
    }
}

#[allow(clippy::redundant_closure_for_method_calls)]
impl From<muon::Error> for ApiServiceError {
    fn from(e: muon::Error) -> Self {
        use MuonErrorKind::*;

        // Check if the error is the result of a timeout.
        if e.source().is_some_and(|s| s.is::<Timeout>()) {
            return Self::Timeout(e.to_string());
        }

        // Check if the error is a HTTP status error.
        if let Some(e) = e.source().and_then(|s| s.downcast_ref::<StatusErr>()) {
            return Self::from(e.to_owned());
        }

        if let Some(e) = e.source().and_then(|s| s.downcast_ref::<AuthErr>()) {
            return Self::Unauthorized(e.to_string(), None);
        }

        // Otherwise, match on the kind of error we received.
        match e.kind() {
            // Connection errors.
            Tls | Resolve | Dial | Connect => Self::ConnectionError(e.to_string()),

            // Network errors.
            Send => Self::NetworkError(e.to_string()),

            // Request errors.
            Req => Self::RequestError(e.to_string()),

            // Response errors.
            Res => Self::ResponseError(e.to_string()),

            // All other errors.
            _ => Self::UnknownError(e.to_string()),
        }
    }
}

impl From<StatusErr> for ApiServiceError {
    fn from(value: StatusErr) -> Self {
        Self::from(&value)
    }
}

impl From<&StatusErr> for ApiServiceError {
    fn from(&StatusErr(code, ref res): &StatusErr) -> Self {
        macro_rules! err {
            ($body:expr) => {
                ApiErrorInfo::from_json($body).ok()
            };
        }

        let body = match String::from_utf8(res.body().to_owned()) {
            Ok(b) => b,
            Err(e) => return Self::Utf8DecodingError(e),
        };

        match (code, code.to_string()) {
            (code, e) if code.is_redirection() => Self::Redirect(e, body),

            (Status::BAD_REQUEST, e) => Self::BadRequest(e, err!(body)),
            (Status::UNAUTHORIZED, e) => Self::Unauthorized(e, err!(body)),
            (Status::FORBIDDEN, e) => Self::Forbidden(e, err!(body)),
            (Status::NOT_FOUND, e) => Self::NotFound(e, err!(body)),
            (Status::UNPROCESSABLE_ENTITY, e) => Self::UnprocessableEntity(e, err!(body)),
            (Status::TOO_MANY_REQUESTS, e) => Self::TooManyRequests(e, err!(body)),
            (Status::INTERNAL_SERVER_ERROR, e) => Self::InternalServerError(e, err!(body)),
            (Status::NOT_IMPLEMENTED, e) => Self::NotImplemented(e, err!(body)),
            (Status::BAD_GATEWAY, e) => Self::BadGateway(e, err!(body)),
            (Status::SERVICE_UNAVAILABLE, e) => Self::ServiceUnavailable(e, err!(body)),

            (code, e) => Self::OtherHttpError(code, e, err!(body)),
        }
    }
}

pub fn deserialize_bool_from_string<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: String = Deserialize::deserialize(deserializer)?;
    match value.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(serde::de::Error::custom(format!(
            "expected \"true\" or \"false\", found \"{value}\"",
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct TestStruct {
        #[serde(deserialize_with = "deserialize_bool_from_string")]
        value: bool,
    }

    #[test]
    fn test_deserialize_bool_from_string_true() {
        let json = r#"{"value": "true"}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        assert!(result.value);
    }

    #[test]
    fn test_deserialize_bool_from_string_false() {
        let json = r#"{"value": "false"}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        assert!(!result.value);
    }

    #[test]
    fn test_deserialize_bool_from_string_invalid() {
        let json = r#"{"value": "invalid"}"#;
        let result: Result<TestStruct, _> = serde_json::from_str(json);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("expected \"true\" or \"false\"")
        );
    }
}
