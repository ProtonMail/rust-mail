use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;
use crate::values::time::{is_hour_value, is_minute_value};

/// Representation of utc-offset value from vCard RFC6350
#[derive(Clone, Debug, PartialEq)]
pub struct UTCOffset(i8, Option<u8>);

impl UTCOffset {
    /// Create a new `utc_offset` (without minute)
    #[must_use]
    pub fn new(hour: i8) -> Self {
        Self(hour, None)
    }

    /// Create a new `utc_offset` (with minute)
    #[must_use]
    pub fn new_with_minute(hour: i8, minute: u8) -> Self {
        Self(hour, Some(minute))
    }
}

impl TryFrom<&str> for UTCOffset {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        match value.len() {
            3 => {
                let value = value.parse().map_err(|_| {
                    VCardValueError::Invalid(ValueType::UTCOffset, value.to_owned())
                })?;
                Ok(Self(value, None))
            }
            5 => {
                let hour = value[0..3].parse().map_err(|_| {
                    VCardValueError::Invalid(ValueType::UTCOffset, value.to_owned())
                })?;
                let minute = value[3..].parse().map_err(|_| {
                    VCardValueError::Invalid(ValueType::UTCOffset, value.to_owned())
                })?;
                Ok(Self(hour, Some(minute)))
            }
            _ => Err(VCardValueError::Invalid(
                ValueType::UTCOffset,
                value.to_owned(),
            )),
        }
    }
}

/// Validate that given `value` respect format for `utc-offset` values
#[must_use]
pub fn is_utc_offset_value(value: &str) -> bool {
    // utc-offset = sign hour [minute]
    let result =
        value.len() > 2 && ["-", "+"].contains(&&value[0..1]) && is_hour_value(&value[1..3]);
    match value.len() {
        3 => result,
        5 => result && is_minute_value(&value[3..5]),
        _ => false,
    }
}
