use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;
use crate::values::time::{is_hour_value, is_minute_value};

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ZoneValue {
    Utc,
    Offset(i8, Option<u8>),
}

impl TryFrom<&str> for ZoneValue {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        if value == "Z" || value == "z" {
            Ok(Self::Utc)
        } else {
            match value.len() {
                3 => Ok(Self::Offset(
                    value.parse().map_err(|_| into_error(value))?,
                    None,
                )),
                5 => Ok(Self::Offset(
                    value[0..3].parse().map_err(|_| into_error(value))?,
                    Some(value[3..5].parse().map_err(|_| into_error(value))?),
                )),
                _ => Err(into_error(value)),
            }
        }
    }
}

fn into_error(value: &str) -> VCardValueError {
    VCardValueError::Invalid(ValueType::TimeZone, value.to_owned())
}

/// Validate that given `value` respect format for `zone` values
#[must_use]
pub fn is_zone_value(value: &str) -> bool {
    // zone   = utc-designator / utc-offset
    // utc-designator = %x5A  ; uppercase "Z"
    // utc-offset = sign hour [minute]
    match value.len() {
        1 => value == "Z",
        3 => ["-", "+"].contains(&&value[0..1]) && is_hour_value(&value[1..3]),
        5 => {
            ["-", "+"].contains(&&value[0..1])
                && is_hour_value(&value[1..3])
                && is_minute_value(&value[3..5])
        }
        _ => false,
    }
}
