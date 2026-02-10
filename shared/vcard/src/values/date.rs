use chrono::NaiveDate;

use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;

// TODO: transform into an enum with all 6 cases
#[derive(Clone, Debug)]
pub struct DateValue {
    pub(crate) year: Option<u16>,
    pub(crate) month: Option<u8>,
    pub(crate) day: Option<u8>,
}

impl DateValue {
    /// Try to create a new date value using vCard format:
    ///  `| year    [month  day]`
    ///  `| year "-" month`
    ///  `| "--"     month [day]`
    ///  `| "--"      "-"   day`
    /// with year on 4 char, month 2 and day 2
    pub fn new_validated(value: &str) -> VCardValueResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for DateValue {
    type Error = VCardValueError;

    fn try_from(value: &str) -> VCardValueResult<Self> {
        fn into_year(value: &str) -> VCardValueResult<u16> {
            value.parse().map_err(|_| into_error(value))
        }

        fn into_month(value: &str) -> VCardValueResult<u8> {
            value.parse().map_err(|_| into_error(value)).and_then(|v| {
                if v > 0 && v < 13 {
                    Ok(v)
                } else {
                    Err(into_error(value))
                }
            })
        }

        fn into_day(value: &str) -> VCardValueResult<u8> {
            value.parse().map_err(|_| into_error(value)).and_then(|v| {
                if v < 32 {
                    Ok(v)
                } else {
                    Err(into_error(value))
                }
            })
        }

        // date          = year    [month  day]  ; 4 || 8
        //               / year "-" month        ; 7
        //               / "--"     month [day]  ; 4 || 6
        //               / "--"      "-"   day   ; 5
        match value.len() {
            4 if &value[0..2] == "--" => Ok(Self {
                year: None,
                month: Some(into_month(&value[2..4])?),
                day: None,
            }),
            4 => Ok(Self {
                year: Some(into_year(&value[0..4])?),
                month: None,
                day: None,
            }),
            5 if &value[0..3] == "---" => Ok(Self {
                year: None,
                month: None,
                day: Some(into_day(&value[3..5])?),
            }),
            6 if &value[0..2] == "--" => Ok(Self {
                year: None,
                month: Some(into_month(&value[2..4])?),
                day: Some(into_day(&value[4..6])?),
            }),
            7 if &value[4..5] == "-" => Ok(Self {
                year: Some(into_year(&value[0..4])?),
                month: Some(into_month(&value[5..7])?),
                day: None,
            }),
            8 => Ok(Self {
                year: Some(into_year(&value[0..4])?),
                month: Some(into_month(&value[4..6])?),
                day: Some(into_day(&value[6..8])?),
            }),
            _ => Err(into_error(value)),
        }
    }
}

fn into_error(value: &str) -> VCardValueError {
    VCardValueError::Invalid(ValueType::Date, value.to_owned())
}

/// Validate that given `value` respect format for `date` values
#[must_use]
pub fn is_date_value(value: &str) -> bool {
    // date          = year    [month  day]  ; 4 || 8
    //               / year "-" month        ; 7
    //               / "--"     month [day]  ; 4 || 6
    //               / "--"      "-"   day   ; 5
    match value.len() {
        4 => is_year_value(value) || (&value[0..2] == "--" && is_month_value(&value[2..4])),
        5 => &value[0..3] == "---" && is_day_value(&value[3..5]),
        6 => &value[0..2] == "--" && is_month_and_day_value(&value[2..]),
        7 => is_year_value(&value[0..4]) && &value[4..5] == "-" && is_month_value(&value[5..7]),
        8 => is_date_complete_value(value),
        _ => false,
    }
}

/// Validate that given `value` respect format for `year` values
pub(crate) fn is_year_value(value: &str) -> bool {
    // year   = 4DIGIT  ; 0000-9999
    value.len() == 4 && value.parse::<u32>().is_ok()
}

/// Validate that given `value` respect format for `month` values
pub(crate) fn is_month_value(value: &str) -> bool {
    // month  = 2DIGIT  ; 01-12
    value.len() == 2 && value.parse::<u32>().is_ok_and(|v| v < 13 && v > 0)
}

/// Validate that given `value` respect format for `hour` values
pub(crate) fn is_day_value(value: &str) -> bool {
    // day    = 2DIGIT  ; 01-28/29/30/31 depending on month and leap year
    value.len() == 2 && value.parse::<u32>().is_ok_and(|v| v > 0 && v < 32)
}

/// Validate that given `value` respect format for a `month` followed by a `day` values
/// Only to be used when no `year` is present
pub(crate) fn is_month_and_day_value(value: &str) -> bool {
    // month  = 2DIGIT  ; 01-12
    // day    = 2DIGIT  ; 01-28/29/30/31 depending on month and leap year
    if value.len() != 4 {
        return false;
    }

    match &value[..2] {
        // January, March, May, July, August, October, December => 1 to 31 days
        "01" | "03" | "05" | "07" | "08" | "10" | "12" => {
            value[2..4].parse::<u32>().is_ok_and(|v| v > 0 && v < 32)
        }
        // February => at most 29 (need to knox the year to be more precise)
        "02" => value[2..4].parse::<u32>().is_ok_and(|v| v > 0 && v < 30),
        // April, June, September, November => 1 to 30 days
        "04" | "06" | "09" | "11" => value[2..4].parse::<u32>().is_ok_and(|v| v > 0 && v < 31),
        _month => false,
    }
}

/// Validate that given `value` respect format for `date-complete` values
pub(crate) fn is_date_complete_value(value: &str) -> bool {
    // date-complete = year     month  day
    if value.len() != 8 {
        return false;
    }

    let Ok(year) = value[..4].parse() else {
        return false;
    };
    let Ok(month) = value[4..6].parse() else {
        return false;
    };
    let Ok(day) = value[6..].parse() else {
        return false;
    };
    NaiveDate::from_ymd_opt(year, month, day).is_some()
}

/// Validate that given `value` respect format for `date-noreduc` values
pub(crate) fn is_date_noreduc_value(value: &str) -> bool {
    // date-noreduc  = year     month  day  ; 8
    //               / "--"     month  day  ; 6
    //               / "--"      "-"   day  ; 5
    match value.len() {
        5 => &value[0..3] == "---" && is_day_value(&value[3..5]),
        6 => &value[0..2] == "--" && is_month_and_day_value(&value[2..6]),
        8 => is_date_complete_value(value),
        _ => false,
    }
}
