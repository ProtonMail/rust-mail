use std::fmt::{Display, Formatter};

use regex::Regex;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

/// Representation of a param-value value from vCard RFC6350
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ParamValue {
    pub(crate) value: String,
}

impl ParamValue {
    /// Create a new param-value (no check is done)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self {
            value: value.to_owned(),
        }
    }

    /// Try to create a new param-value
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl Display for ParamValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "PV({})", self.value)
    }
}

impl TryFrom<&str> for ParamValue {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        if is_param_value(value) {
            Ok(Self::new_unchecked(value))
        } else if value.find('"').is_none() {
            // TODO: is it a good idea ?
            // adding double quote to make it valid
            Ok(Self {
                value: format!(r#""{value}""#),
            })
        } else {
            Err(VCardValueError::Invalid(
                ValueType::ParamValue,
                value.to_owned(),
            ))
        }
    }
}

/// Validate that the given `value` respect the format for a `param-value`
#[allow(clippy::similar_names)]
#[must_use]
pub fn is_param_value(value: &str) -> bool {
    // param-value = *SAFE-CHAR / DQUOTE *QSAFE-CHAR DQUOTE
    // SAFE-CHAR = WSP / "!" / %x23-39 / %x3C-7E / NON-ASCII
    //  ; Any character except CTLs, DQUOTE, ";", ":"
    // QSAFE-CHAR = WSP / "!" / %x23-7E / NON-ASCII
    //  ; Any character except CTLs, DQUOTE
    let re_safe = Regex::new("^([ \t!\x23-\x39\x3C-\x7E]|[^\x00-\x7F])*$").unwrap();
    let re_qsafe = Regex::new("^\x22([ \t!\x23-\x7E]|[^\x00-\x7F])*\x22$").unwrap();
    re_safe.is_match(value) || re_qsafe.is_match(value)
}
