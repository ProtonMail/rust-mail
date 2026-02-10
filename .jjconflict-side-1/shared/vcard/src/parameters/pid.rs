use std::fmt::Debug;

use crate::{
    ParameterType,
    errors::{VCardParameterError, VCardParameterResult},
};
use regex::Regex;

/// The PID parameter is used to identify a specific property among multiple instances.
#[derive(Debug, Clone)]
pub struct Pid {
    /// Values
    pub values: Vec<PidElement>,
}

impl Pid {
    /// Try to create a new PID parameter
    pub fn new_validated(values: &[String]) -> VCardParameterResult<Self> {
        Self::try_from(values)
    }
}

impl TryFrom<&[String]> for Pid {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if is_pid_param(values) {
            Ok(Self {
                values: values
                    .iter()
                    .map(|v| PidElement::try_from(v.as_str()))
                    .collect::<Result<_, _>>()?,
            })
        } else {
            Err(VCardParameterError::InvalidValues(
                ParameterType::Pid,
                values.to_vec(),
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub struct PidElement(pub u32, pub Option<u32>);

impl TryFrom<&str> for PidElement {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        fn error(value: &str) -> VCardParameterError {
            VCardParameterError::InvalidValue(ParameterType::Pid, value.to_owned())
        }

        if let Some((start, end)) = value.split_once('.') {
            let start = start.parse().map_err(|_| error(value))?;
            let end = end.parse().map_err(|_| error(value))?;
            Ok(Self(start, Some(end)))
        } else {
            let value = value.parse().map_err(|_| error(value))?;
            Ok(Self(value, None))
        }
    }
}

/// Validate that the given `values` respect the format for a `PID` parameter
#[must_use]
pub fn is_pid_param(values: &[String]) -> bool {
    // pid-param = "PID=" pid-value *("," pid-value)
    // pid-value = 1*DIGIT ["." 1*DIGIT]
    let re = Regex::new("^[0-9]+([.][0-9]+)?$").unwrap();
    !values.is_empty()
        && values.iter().all(|v| {
            re.captures(v).is_some_and(|v| {
                let source = v.get(1);
                source.is_none()
                    || source.is_some_and(|v| v.as_str()[1..].parse::<u32>().is_ok_and(|v| v > 0))
            })
        })
}
