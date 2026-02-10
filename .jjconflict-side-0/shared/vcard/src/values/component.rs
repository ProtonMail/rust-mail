use std::fmt::Display;

use regex::Regex;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

/// Representation of a `component` value from vCard
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Component(pub String);

impl Display for Component {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Component {
    /// Create a new `Component` from a str (no check are done)
    #[must_use]
    pub fn new(value: &str) -> Self {
        Self(value.to_owned())
    }

    /// Try to create a new `Component` from a str comming from a vCard
    pub fn new_from_vcard(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for Component {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        if is_component_value(value) {
            Ok(Self(unescape(value)))
        } else {
            Err(VCardValueError::Invalid(
                ValueType::Component,
                value.to_owned(),
            ))
        }
    }
}

/// Validate that given `value` respect format for `component` values
#[must_use]
pub fn is_component_value(value: &str) -> bool {
    // component = "\\" / "\," / "\;" / "\n" / WSP / NON-ASCII / %x21-2B / %x2D-3A / %x3C-5B / %x5D-7E
    // /!\ that line don't make sense in itself, components can have more than one char /!\
    let re = Regex::new(
        r"^(\\\\|\\,|\\;|\\n|[ \t]|[^\x00-\x7F]|[\x21-\x2B]|[\x2D-\x3A]|[\x3C-\x5B]|[\x5D-\x7E])*$",
    )
    .unwrap();
    re.is_match(value)
}

fn unescape(value: &str) -> String {
    value
        .replace(r"\,", ",")
        .replace(r"\;", ";")
        .replace(r"\n", "\n")
        .replace(r"\\", r"\")
}
