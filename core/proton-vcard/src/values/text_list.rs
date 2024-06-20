use std::fmt::{Debug, Formatter};

use crate::errors::{VCardValueError, VCardValueResult};
use crate::values::check_list;
use crate::values::text::{is_text_value, Text};
use crate::vcard::split_list;

/// Represent a text-list value from vCard RFC6350
#[derive(Clone, PartialEq)]
pub struct TextList(pub(crate) Vec<Text>);

impl TextList {
    /// Create a new text-list from given values (no check are done)
    #[must_use]
    pub fn new_unchecked(value: &[String]) -> Self {
        Self(
            value
                .iter()
                .map(|v| Text::new_unchecked(v.as_str()))
                .collect(),
        )
    }

    /// Try to create a new text-list from given values
    ///
    /// # Errors
    ///   * if at least one of the values is not a valid text value
    pub fn new_validated(values: &[String]) -> VCardValueResult<Self> {
        Ok(Self(
            values
                .iter()
                .map(|v| Text::new_validated(v))
                .collect::<Result<_, _>>()?,
        ))
    }
    /// Try to create a new text-list from given values
    ///
    /// # Errors
    ///   * if at least one of the values is not a valid text value
    pub fn new_from_vcard(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl Debug for TextList {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "TL({:?})", self.0)
    }
}

impl TryFrom<&str> for TextList {
    type Error = VCardValueError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let values = split_list(value, ',')
            .iter()
            .map(|v| Text::new_validated(v.as_str()))
            .collect::<Result<_, _>>()?;
        Ok(Self(values))
    }
}

/// Validate that given `value` respect format for `text-list` values
pub fn is_text_list_value(value: &str) -> bool {
    // text-list             = text             *("," text)
    // text = *TEXT-CHAR
    // TEXT-CHAR = "\\" / "\," / "\n" / WSP / NON-ASCII / %x21-2B / %x2D-5B / %x5D-7E
    //    ; Backslashes, commas, and newlines must be encoded.

    check_list(value, is_text_value, ',').is_some()
}
