use crate::errors::{VCardParameterError, VCardParameterResult};
use std::fmt::Debug;

use crate::ParameterType;
use crate::values::param_value::{ParamValue, is_param_value};

/// The ALTID parameter is used to "tag" property instances as being alternative representations of
/// the same logical property.  For example, translations of a property in multiple languages
/// generates multiple property instances having different LANGUAGE parameter that are tagged with
/// the same ALTID value.
#[derive(Debug, Clone)]
pub struct AlternativeId {
    /// value
    pub value: ParamValue,
}

impl AlternativeId {
    /// Create a new ALTID parameter (no check is done)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self {
            value: ParamValue::new_unchecked(value),
        }
    }

    /// Try to create a new ALTID parameter
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Ok(Self {
            value: ParamValue::try_from(value)
                .map_err(VCardParameterError::from_value_error(ParameterType::AltId))?,
        })
    }
}

impl From<ParamValue> for AlternativeId {
    fn from(value: ParamValue) -> Self {
        Self { value }
    }
}

impl TryFrom<&str> for AlternativeId {
    type Error = VCardParameterError;
    fn try_from(value: &str) -> VCardParameterResult<Self> {
        Ok(Self {
            value: ParamValue::try_from(value)
                .map_err(VCardParameterError::from_value_error(ParameterType::AltId))?,
        })
    }
}

impl TryFrom<&[String]> for AlternativeId {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() == 1 {
            Ok(Self::try_from(values[0].as_str())?)
        } else {
            Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::AltId,
                values.to_vec(),
            ))
        }
    }
}

/// Validate that the given `values` respect the format for a `ALTID` parameter
#[must_use]
pub fn is_altid_param(values: &[String]) -> bool {
    // altid-param = "ALTID=" param-value
    values.len() == 1 && is_param_value(&values[0])
}
