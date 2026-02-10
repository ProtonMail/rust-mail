use std::fmt::Debug;

use url::Url;

use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};
use crate::values::param_value::{ParamValue, is_param_value};
use crate::values::uri::Uri;

/// The TZ parameter can be used to indicate time zone information that is specific to an address.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeZone {
    Uri(Uri),
    ParamValue(ParamValue),
}

impl TimeZone {
    /// Create a new TZ parameter from a URL
    #[must_use]
    pub fn new_uri(value: Url) -> Self {
        Self::Uri(Uri::new(value))
    }

    /// Try to create a new TZ parameter from a str
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&[String]> for TimeZone {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() != 1 {
            return Err(VCardParameterError::InvalidValues(
                ParameterType::TZ,
                values.to_vec(),
            ));
        }
        Self::try_from(values[0].as_str())
    }
}

impl TryFrom<&str> for TimeZone {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        if let Ok(value) = Url::parse(value) {
            Ok(Self::Uri(Uri::new(value)))
        } else if is_param_value(value) {
            Ok(Self::ParamValue(ParamValue::try_from(value).map_err(
                VCardParameterError::from_value_error(ParameterType::TZ),
            )?))
        } else {
            Err(VCardParameterError::InvalidValue(
                ParameterType::TZ,
                value.to_owned(),
            ))
        }
    }
}

/// Validate that the given `values` respect the format for a `TZ` parameter
#[must_use]
pub fn is_tz_param(values: &[String]) -> bool {
    // TODO: check if ICal do remove the double quote
    // tz-parameter = "TZ=" (param-value / DQUOTE URI DQUOTE)
    values.len() == 1
        && (is_param_value(&values[0]) || {
            let value: &str = &values[0];
            // URI               ; from Section 3 of [RFC3986]
            Url::parse(value).is_ok()
        })
}
