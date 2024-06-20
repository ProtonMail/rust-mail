use std::fmt::{Debug, Formatter};

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;
use crate::values::date::{is_date_value, DateValue};
use crate::values::date_time::{is_date_time_value, DateTimeValue};
use crate::values::time::{is_time_value, TimeValue};

// TODO: transform into an enum with all 3 cases
/// Representation of a date-and-or-time value from vCard RFC6350
#[derive(Clone, Copy, PartialEq)]
pub struct DateAndOrTimeValue(pub(crate) DateTimeValue);

impl DateAndOrTimeValue {
    /// Try to create a new `DateAndOrTimeValue`
    ///
    /// # Errors
    ///   * if given value is not valid (see RFC6350 4.3.4 for valid formats)
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl Debug for DateAndOrTimeValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write! {f, "DaoT({:?})", self.0}
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
