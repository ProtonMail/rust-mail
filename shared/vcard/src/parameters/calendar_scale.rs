use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};
use crate::values::iana_token::{IanaToken, is_iana_token_value};
use crate::values::x_name::{XName, is_x_name_value};
use std::fmt::Debug;

const GREGORIAN: &str = "gregorian";

/// The CALSCALE parameter is identical to the CALSCALE property in iCalendar (see (RFC5545),
/// Section 3.7.1).  It is used to define the calendar system in which a date or date-time value is
/// expressed.
#[derive(Debug, Clone, PartialEq)]
pub enum CalendarScale {
    /// Gregorian calendar
    Gregorian,
    /// Iana token
    IanaToken(IanaToken),
    /// X-name
    XName(XName),
}

impl CalendarScale {
    #[must_use]
    /// Create a new CALSCALE parameter (no check is done)
    pub fn new_unchecked(value: &str) -> Self {
        if value == GREGORIAN {
            Self::Gregorian
        } else if is_x_name_value(value) {
            Self::XName(XName::new_unchecked(value))
        } else {
            Self::IanaToken(IanaToken::new_unchecked(value))
        }
    }

    /// Try to create a new CALSCALE parameter
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&[String]> for CalendarScale {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() != 1 {
            return Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::CalScale,
                values.to_vec(),
            ));
        }
        Self::try_from(values[0].as_str())
    }
}

impl TryFrom<&str> for CalendarScale {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        match value.to_ascii_lowercase().as_str() {
            GREGORIAN => Ok(Self::Gregorian),
            value if value.starts_with("x-") || value.starts_with("X-") => {
                Ok(Self::XName(XName::try_from(value).map_err(
                    VCardParameterError::from_value_error(ParameterType::CalScale),
                )?))
            }
            value => Ok(Self::IanaToken(IanaToken::try_from(value).map_err(
                VCardParameterError::from_value_error(ParameterType::CalScale),
            )?)),
        }
    }
}

/// Validate that the given `values` respect the format for a `CALSCALE` parameter
#[must_use]
pub fn is_calscale_param(values: &[String]) -> bool {
    // calscale-param = "CALSCALE=" calscale-value
    // calscale-value = "gregorian" / iana-token / x-name
    values.len() == 1
        && (values[0].to_ascii_lowercase() == GREGORIAN
            || is_iana_token_value(&values[0])
            || is_x_name_value(&values[0]))
}
