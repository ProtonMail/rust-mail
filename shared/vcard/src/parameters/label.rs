use std::fmt::Debug;

use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};
use crate::values::param_value::{ParamValue, is_param_value};

/// An ADR can include a "LABEL" parameter to present a delivery address label for the address.  Its
/// value is a plain-text string representing the formatted address.
#[derive(Clone, Debug)]
pub struct Label {
    /// Value
    pub value: ParamValue,
}

impl Label {
    /// Create a new Label from a str (no check are done to verify that the value is a valid
    /// param-value)
    #[must_use]
    pub fn new_unchecked(value: &str) -> Self {
        Self {
            value: ParamValue::new_unchecked(value),
        }
    }

    /// Try to create a new LABEL parameter
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Ok(Self {
            value: ParamValue::try_from(value)
                .map_err(VCardParameterError::from_value_error(ParameterType::Label))?,
        })
    }
}

impl TryFrom<&[String]> for Label {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() != 1 {
            return Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::Label,
                values.to_vec(),
            ));
        }
        Ok(Self {
            value: ParamValue::try_from(values[0].as_str())
                .map_err(VCardParameterError::from_value_error(ParameterType::Label))?,
        })
    }
}

/// Validate that the given `values` respect the format for a `LABEL` parameter
#[must_use]
pub fn is_label_param(values: &[String]) -> bool {
    // label-param = "LABEL=" param-value
    values.len() == 1 && is_param_value(&values[0])
}
