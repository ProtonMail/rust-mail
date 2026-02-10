use std::fmt::Debug;

use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};
use crate::values::param_value::{ParamValue, is_param_value};

/// The "sort-as" parameter is used to specify the string to be used for national-language-specific
/// sorting.
#[derive(Debug, Clone)]
pub struct SortAs {
    /// Value
    pub values: Vec<ParamValue>,
}

impl SortAs {
    /// Create a new sort-as parameter (No check done on given values)
    #[must_use]
    pub fn new_unchecked(values: &[String]) -> Self {
        Self {
            values: values
                .iter()
                .map(|v| ParamValue::new_unchecked(v.as_str()))
                .collect(),
        }
    }

    /// Try to create a new sort-as parameter
    pub fn new_validated(values: &[String]) -> VCardParameterResult<Self> {
        Ok(Self {
            values: values
                .iter()
                .map(|v| ParamValue::try_from(v.as_str()))
                .collect::<Result<_, _>>()
                .map_err(VCardParameterError::from_value_error(ParameterType::SortAs))?,
        })
    }
}

impl TryFrom<&[String]> for SortAs {
    type Error = VCardParameterError;
    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        Ok(Self {
            values: values
                .iter()
                .map(|v| ParamValue::try_from(v.as_str()))
                .collect::<Result<_, _>>()
                .map_err(VCardParameterError::from_value_error(ParameterType::SortAs))?,
        })
    }
}

/// Validate that the given `values` respect the format for a `SORT-AS` parameter
#[must_use]
pub fn is_sort_as_param(values: &[String]) -> bool {
    // sort-as-param = "SORT-AS=" sort-as-value
    // sort-as-value = param-value *("," param-value)
    !values.is_empty() && values.iter().all(|v| is_param_value(v))
}
