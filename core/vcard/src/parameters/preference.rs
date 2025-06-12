use std::fmt::Debug;

use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};

const MIN_PREF: u32 = 1;
const MAX_PREF: u32 = 100;
const LOWEST_PREF: u32 = 101;

/// The PREF parameter is OPTIONAL and is used to indicate that the corresponding instance of a
/// property is preferred by the vCard author.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub struct Preference {
    pub value: u32,
}

impl Preference {
    #[must_use]
    pub fn new(value: u32) -> Self {
        Self { value }
    }

    #[must_use]
    pub fn is_valid_value(&self) -> bool {
        (MIN_PREF..=MAX_PREF).contains(&self.value)
    }

    #[must_use]
    pub fn less_than_lowest() -> Self {
        Self { value: LOWEST_PREF }
    }
}

impl From<u32> for Preference {
    fn from(value: u32) -> Self {
        Self { value }
    }
}

impl TryFrom<&[String]> for Preference {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() == 1 {
            Self::try_from(values[0].as_str())
        } else {
            Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::Pref,
                values.to_vec(),
            ))
        }
    }
}

impl TryFrom<&str> for Preference {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        Ok(Self {
            value: value.parse().map_err(|_| {
                VCardParameterError::InvalidValue(ParameterType::Pref, value.to_owned())
            })?,
        })
    }
}

/// Validate that the given `values` respect the format for a `PREF` parameter
#[must_use]
pub fn is_pref_param(values: &[String]) -> bool {
    // pref-param = "PREF=" (1*2DIGIT / "100")
    //            ; An integer between 1 and 100.
    values.len() == 1 && values[0].parse::<u32>().is_ok_and(|v| v > 0 && v < 101)
}
