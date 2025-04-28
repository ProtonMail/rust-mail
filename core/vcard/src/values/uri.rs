use std::fmt::{Debug, Formatter};

use url::Url;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

/// A Uri as defined in (RFC3986)
#[derive(Clone, PartialEq)]
pub struct Uri(pub Url);

impl Uri {
    /// Create a new Uri from a URL
    #[must_use]
    pub fn new(value: Url) -> Self {
        Self(value)
    }

    /// Try to create a new URI from a str
    ///
    /// # Errors
    ///   * if given value is not a valid URL
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl Debug for Uri {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Uri({})", self.0)
    }
}

impl TryFrom<&str> for Uri {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        Ok(Self(Url::parse(value).map_err(|_| {
            VCardValueError::Invalid(ValueType::Uri, value.to_owned())
        })?))
    }
}

/// Validate that given `value` respect format for `URI` values
#[must_use]
pub fn is_uri_value(value: &str) -> bool {
    // URI               ; from Section 3 of [RFC3986]
    Url::parse(value).is_ok()
}
