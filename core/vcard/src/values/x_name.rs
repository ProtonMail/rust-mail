use std::fmt::{Debug, Formatter};

use regex::Regex;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

/// Representation for the x-name values from vCard RFC6350
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct XName(pub String);

impl Debug for XName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "XN({})", self.0)
    }
}

impl XName {
    /// Create a new x-name value (no check is done)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self(value.to_owned())
    }

    /// Try to create a new x-name value
    ///
    /// # Errors
    ///   * if the given value is not a valid x-name (start with x- and alphanumerical char + dash)
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for XName {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        if is_x_name_value(value) {
            Ok(Self::new_unchecked(value))
        } else {
            Err(VCardValueError::Invalid(ValueType::XName, value.to_owned()))
        }
    }
}

/// Validate that given `value` respect format for `x-name` values
#[must_use]
pub fn is_x_name_value(value: &str) -> bool {
    // x-name = "x-" 1*(ALPHA / DIGIT / "-")
    //  ; Names that begin with "x-" or "X-" are
    //  ; reserved for experimental use, not intended for released
    //  ; products, or for use in bilateral agreements.
    let re = Regex::new("^[xX]-[a-zA-Z0-9-]+$").unwrap();
    re.is_match(value)
}
