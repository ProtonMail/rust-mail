use std::fmt::{Display, Formatter};

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;
use crate::values::date::{DateValue, is_date_value};
use crate::values::date_time::{DateTimeValue, is_date_time_value};
use crate::values::time::{TimeValue, is_time_value};

// TODO: transform into an enum with all 3 cases
/// Representation of a date-and-or-time value from vCard RFC6350
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateAndOrTimeValue(pub DateTimeValue);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaybeDateAndOrTime {
    DateAndOrTime(DateAndOrTimeValue),
    Text(String),
}
impl Default for MaybeDateAndOrTime {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl Display for MaybeDateAndOrTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(s) => write!(f, "{s}")?,
            Self::DateAndOrTime(date) => {
                if let Some(year) = date.0.year {
                    write!(f, "{year}")?;
                } else {
                    write!(f, "?")?;
                }
                if let Some(month) = date.0.month {
                    write!(f, "/{month}")?;
                } else {
                    write!(f, "/?")?;
                }
                if let Some(day) = date.0.day {
                    write!(f, "/{day}")?;
                } else {
                    write!(f, "/?")?;
                }
            }
        }
        Ok(())
    }
}

impl<T: AsRef<str>> From<T> for MaybeDateAndOrTime {
    fn from(value: T) -> Self {
        let value = value.as_ref();
        match DateAndOrTimeValue::try_from(value) {
            Ok(v) => Self::DateAndOrTime(v),
            _ => Self::Text(value.into()),
        }
    }
}

impl DateAndOrTimeValue {
    /// Try to create a new `DateAndOrTimeValue`
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for DateAndOrTimeValue {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        if value.is_empty() {
            return Err(into_error(value));
        }
        if &value[0..1] == "T" {
            let time = TimeValue::try_from(&value[1..])?;
            Ok(Self(DateTimeValue::from(time)))
        } else if let Some(position) = value.find('T') {
            let date = DateValue::try_from(&value[0..position])?;
            let time = TimeValue::try_from(&value[position + 1..])?;
            Ok(Self(DateTimeValue::from_date_and_time(&date, &time)))
        } else {
            let date = DateValue::try_from(value)?;
            Ok(Self(DateTimeValue::from(date)))
        }
    }
}

fn into_error(value: &str) -> VCardValueError {
    VCardValueError::Invalid(ValueType::DateAndOrTime, value.to_owned())
}

/// Validate that given `value` respect format for `date-and-or-time` values
#[must_use]
pub fn is_date_and_or_time_value(value: &str) -> bool {
    // date-and-or-time = date-time / date / time-designator time
    // time-designator = %x54  ; uppercase "T"
    is_date_time_value(value)
        || is_date_value(value)
        || (value.len() > 2 && &value[0..1] == "T" && is_time_value(&value[1..]))
}
