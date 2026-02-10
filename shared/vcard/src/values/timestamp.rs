use std::fmt::Debug;

use chrono::{DateTime, FixedOffset, MappedLocalTime, TimeZone};

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;
use crate::values::date::is_date_complete_value;
use crate::values::time::is_time_complete_value;

const MINUTE: i32 = 60;
const HOUR: i32 = 60 * MINUTE;

/// A representation of the timestamp value from vCard RFC6350
#[derive(Clone, PartialEq, Debug)]
pub struct Timestamp(pub(crate) DateTime<FixedOffset>);

impl Timestamp {
    /// Create a new timestamp value
    #[must_use]
    pub fn new(value: DateTime<FixedOffset>) -> Self {
        Self(value)
    }

    /// Try to create a new timestamp value
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for Timestamp {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        fn into_error(value: &str) -> VCardValueError {
            VCardValueError::Invalid(ValueType::Timestamp, value.to_owned())
        }

        if &value[8..9] != "T" {
            return Err(into_error(value));
        }
        let year = value[0..4].parse().map_err(|_| into_error(value))?;
        let month = value[4..6].parse().map_err(|_| into_error(value))?;
        let day = value[6..8].parse().map_err(|_| into_error(value))?;
        let hour = value[9..11].parse().map_err(|_| into_error(value))?;
        let minute = value[11..13].parse().map_err(|_| into_error(value))?;
        let second = value[13..15].parse().map_err(|_| into_error(value))?;
        let offset = match value.len() {
            // No time zone
            15 => FixedOffset::east_opt(0).ok_or(into_error(value))?,
            // UTC
            16 => {
                if &value[15..16] == "Z" {
                    FixedOffset::east_opt(0).ok_or(into_error(value))?
                } else {
                    return Err(into_error(value));
                }
            }
            // hour
            18 => {
                let offset: i32 = value[15..18].parse().map_err(|_| into_error(value))?;
                FixedOffset::east_opt(offset * HOUR).ok_or(into_error(value))?
            }
            // hour + minute
            20 => {
                let offset: i32 = value[15..18].parse().map_err(|_| into_error(value))?;
                let minute_offset: i32 = value[18..20].parse().map_err(|_| into_error(value))?;
                FixedOffset::east_opt(offset * HOUR + minute_offset * MINUTE)
                    .ok_or(into_error(value))?
            }
            _ => return Err(into_error(value)),
        };
        match offset.with_ymd_and_hms(year, month, day, hour, minute, second) {
            MappedLocalTime::Single(v) | MappedLocalTime::Ambiguous(v, _) => Ok(Self(v)),
            MappedLocalTime::None => Err(into_error(value)),
        }
    }
}

/// Validate that given `value` respect format for `timestamp` values
#[must_use]
pub fn is_timestamp_value(value: &str) -> bool {
    // timestamp = date-complete time-designator time-complete
    // date-complete = year month day
    // time-designator = %x54 ; uppercase "T"
    // time-complete = hour minute second [zone]
    // example: 99991231T235959+2359
    value.len() > 14
        && is_date_complete_value(&value[0..8])
        && &value[8..9] == "T"
        && is_time_complete_value(&value[9..])
}
