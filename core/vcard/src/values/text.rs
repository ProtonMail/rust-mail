use std::fmt::{Debug, Formatter};

use regex::Regex;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

/// Representation of a text value from vCard RFC6350
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct Text {
    pub value: String,
}

impl Text {
    /// Create a new `Text` value from a str (no check are done)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self {
            value: value.to_owned(),
        }
    }

    /// Try to create a new `Text` value from a str
    ///
    /// # Errors
    ///   * if given value is not a valid text
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl Debug for Text {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "T({:?})", self.value)
    }
}

impl TryFrom<&str> for Text {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        if !is_text_value(value) {
            return Err(VCardValueError::Invalid(
                ValueType::ParamValue,
                value.to_owned(),
            ));
        }
        Ok(Self {
            value: unescape(value),
        })
    }
}

/// Validate that given `value` respect format for `text` values
#[allow(clippy::missing_panics_doc, reason = "Valid regex")]
#[must_use]
pub fn is_text_value(value: &str) -> bool {
    // text = *TEXT-CHAR
    // TEXT-CHAR = "\\" / "\," / "\n" / WSP / NON-ASCII / %x21-2B / %x2D-5B / %x5D-7E
    //    ; Backslashes, commas, and newlines must be encoded.
    let re =
        Regex::new(r"^(\\\\|\\,|\\n|[ \t]|[^\x00-\x7F]|[\x21-\x2B]|[\x2D-\x5B]|[\x5D-\x7E])*$")
            .unwrap();
    re.is_match(value)
}

fn unescape(value: &str) -> String {
    value
        .replace(r"\,", ",")
        .replace(r"\n", "\n")
        .replace(r"\\", r"\")
}
