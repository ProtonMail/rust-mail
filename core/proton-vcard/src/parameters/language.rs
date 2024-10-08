use std::fmt::{Debug, Formatter};

use oxilangtag::LanguageTag as OxiLanguageTag;

use crate::errors::{VCardParameterError, VCardParameterResult};
use crate::values::language_tag::{is_language_tag_value, LanguageTag};
use crate::ParameterType;

/// The LANGUAGE property parameter is used to identify data in multiple languages.
#[derive(Clone)]
pub struct Language {
    /// Value
    pub value: LanguageTag,
}

impl Language {
    /// Create a new Language parameter
    #[must_use]
    pub fn new(value: OxiLanguageTag<String>) -> Self {
        Self {
            value: LanguageTag::new(value),
        }
    }

    /// Try to create a new Language parameter
    ///
    /// # Errors
    ///   * given value is not a language-tag
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Ok(Self {
            value: LanguageTag::new_validated(value).map_err(
                VCardParameterError::from_value_error(ParameterType::Language),
            )?,
        })
    }
}

impl Debug for Language {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Language {{{:?}}}", self.value)
    }
}

impl TryFrom<&[String]> for Language {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() != 1 {
            return Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::Language,
                values.to_vec(),
            ));
        }
        Ok(Self {
            value: LanguageTag::try_from(values[0].as_str()).map_err(
                VCardParameterError::from_value_error(ParameterType::Language),
            )?,
        })
    }
}

/// Validate that the given `values` respect the format for a `LANGUAGE` parameter
#[must_use]
pub fn is_language_param(values: &[String]) -> bool {
    // language-param = "LANGUAGE=" Language-Tag
    //          ; Language-Tag is defined in section 2.1 of RFC 5646
    values.len() == 1 && is_language_tag_value(values[0].as_str())
}
