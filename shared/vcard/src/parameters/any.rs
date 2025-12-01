use crate::errors::{VCardParameterError, VCardParameterResult};
use std::fmt::Debug;

use crate::ParameterType;
use crate::values::iana_token::{IanaToken, is_iana_token_value};
use crate::values::param_value::{ParamValue, is_param_value};
use crate::values::x_name::{XName, is_x_name_value};

/// Additional parameter authorized but not defined by RFC6350
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Any {
    /// name of the parameter (Iana token or x-name)
    pub name: AnyName,
    /// one or more param-value
    pub values: Vec<ParamValue>,
}

impl Any {
    /// Create a new parameter out of vCard (but authorized by) specification (no check is done)
    #[must_use]
    pub fn new_unchecked(name: &str, values: &[String]) -> Self {
        if name.starts_with("X-") || name.starts_with("x-") {
            Self {
                name: AnyName::XName(XName::new_unchecked(name)),
                values: values
                    .iter()
                    .map(|v| ParamValue::new_unchecked(v))
                    .collect(),
            }
        } else {
            Self {
                name: AnyName::IanaToken(IanaToken::new_unchecked(name)),
                values: values
                    .iter()
                    .map(|v| ParamValue::new_unchecked(v))
                    .collect(),
            }
        }
    }

    /// Try to create a new parameter out of vCard (but authorized by) specification
    pub fn new_validated(name: &str, values: &[String]) -> VCardParameterResult<Self> {
        if values.is_empty() {
            return Err(VCardParameterError::ExpectedAtLeastOneValue(
                ParameterType::Any,
            ));
        }

        Ok(Self {
            name: AnyName::try_from(name)?,
            values: values
                .iter()
                .map(|v| ParamValue::try_from(v.as_str()))
                .collect::<Result<_, _>>()
                .map_err(VCardParameterError::from_value_error(ParameterType::Any))?,
        })
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub enum AnyName {
    IanaToken(IanaToken),
    XName(XName),
}

impl TryFrom<&str> for AnyName {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        if is_x_name_value(value) {
            Ok(Self::XName(XName::new_unchecked(value)))
        } else if is_iana_token_value(value) {
            Ok(Self::IanaToken(IanaToken::new_unchecked(value)))
        } else {
            Err(VCardParameterError::InvalidName(value.to_owned()))
        }
    }
}

/// Validate that the given `values` and `name` respect the format for an any parameter
///
/// `Values` can be any not empty list of param value
#[must_use]
pub fn is_any_param(name: &str, values: &[String]) -> bool {
    // any-param  = (iana-token / x-name) "=" param-value *("," param-value)
    (is_iana_token_value(name) || is_x_name_value(name))
        && !values.is_empty()
        && values.iter().all(|v| is_param_value(v))
}
