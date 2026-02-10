use std::fmt::{Debug, Display, Formatter};

use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};

/// The LANGUAGE property parameter is used to identify data in multiple languages.
#[derive(Clone, Debug)]
pub struct Language(pub String);

impl Display for Language {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<Vec<String>> for Language {
    type Error = VCardParameterError;

    fn try_from(mut values: Vec<String>) -> VCardParameterResult<Self> {
        let Some(val) = values.pop() else {
            return Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::Language,
                values,
            ));
        };

        Ok(Self(val))
    }
}

/// Validate that the given `values` respect the format for a `LANGUAGE` parameter
#[must_use]
pub fn is_language_param(values: &[String]) -> bool {
    // language-param = "LANGUAGE=" Language-Tag
    //          ; Language-Tag is defined in section 2.1 of RFC 5646
    !values.is_empty()
}
