use std::fmt::{Debug, Formatter};

use oxilangtag::LanguageTag as OxiLanguageTag;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

/// A language tag as defined in RFC5646
#[derive(Clone, PartialEq)]
pub struct LanguageTag(pub(crate) OxiLanguageTag<String>);

impl LanguageTag {
    /// Create a new language-tag value
    #[must_use]
    pub fn new(value: OxiLanguageTag<String>) -> Self {
        Self(value)
    }

    /// Try to create a new language-tag from a str
    ///
    /// # Errors
    ///   * if given value is not a valid language tag
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl Debug for LanguageTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "LT({})", self.0)
    }
}

impl TryFrom<&str> for LanguageTag {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        Ok(Self(OxiLanguageTag::parse(value.to_owned()).map_err(
            |_| VCardValueError::Invalid(ValueType::LanguageTag, value.to_owned()),
        )?))
    }
}

/// Validate that given `value` respect format for `language-tag` values
#[must_use]
pub fn is_language_tag_value(value: &str) -> bool {
    // Language-Tag = <Language-Tag, defined in [RFC5646], Section 2.1>
    OxiLanguageTag::parse(value).is_ok()
}
