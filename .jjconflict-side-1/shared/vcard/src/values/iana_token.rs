use std::fmt::Debug;

use regex::Regex;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

/// Represent a iana-token value from vCard RFC6350
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct IanaToken(pub String);

impl IanaToken {
    /// Create a new iana-token (no check is done)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self(value.to_owned())
    }

    /// Try to create a new iana-token
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for IanaToken {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        if is_iana_token_value(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(VCardValueError::Invalid(
                ValueType::IanaToken,
                value.to_owned(),
            ))
        }
    }
}

/// Validate that given `value` respect format for `iana-token` values
#[must_use]
pub fn is_iana_token_value(value: &str) -> bool {
    // iana-token = 1*(ALPHA / DIGIT / "-")
    //  ; identifier registered with IANA
    let re = Regex::new("^[a-zA-Z0-9-]+$").unwrap();
    re.is_match(value)
}
