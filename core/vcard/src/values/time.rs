use crate::errors::{VCardValueError, VCardValueResult};
use crate::parameters::value::ValueType;
use crate::values::zone::{ZoneValue, is_zone_value};

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug)]
pub struct TimeValue {
    pub(crate) hour: Option<u8>,
    pub(crate) minute: Option<u8>,
    pub(crate) second: Option<u8>,
    pub(crate) zone: Option<ZoneValue>,
}

fn into_hour(value: &str) -> VCardValueResult<u8> {
    value.parse().map_err(|_| into_error(value)).and_then(|v| {
        if v < 25 {
            Ok(v)
        } else {
            Err(into_error(value))
        }
    })
}

fn into_minute(value: &str) -> VCardValueResult<u8> {
    value.parse().map_err(|_| into_error(value)).and_then(|v| {
        if v < 61 {
            Ok(v)
        } else {
            Err(into_error(value))
        }
    })
}

fn into_second(value: &str) -> VCardValueResult<u8> {
    value.parse().map_err(|_| into_error(value)).and_then(|v| {
        // leap second
        if v < 62 {
            Ok(v)
        } else {
            Err(into_error(value))
        }
    })
}

fn into_error(value: &str) -> VCardValueError {
    VCardValueError::Invalid(ValueType::Time, value.to_owned())
}

impl TryFrom<&str> for TimeValue {
    type Error = VCardValueError;

    #[allow(clippy::too_many_lines)]
    fn try_from(value: &str) -> VCardValueResult<Self> {
        match value.len() {
            // HH
            2 => Ok(Self {
                hour: Some(into_hour(value)?),
                minute: None,
                second: None,
                zone: None,
            }),
            // -MM
            3 if &value[0..1] == "-" => Ok(Self {
                hour: None,
                minute: Some(into_minute(&value[1..3])?),
                second: None,
                zone: None,
            }),
            // HHZ
            3 => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: None,
                second: None,
                zone: Some(ZoneValue::try_from(&value[2..3])?),
            }),
            // --SS
            4 if &value[0..2] == "--" => Ok(Self {
                hour: None,
                minute: None,
                second: Some(into_second(&value[2..4])?),
                zone: None,
            }),
            // -MMZ
            4 if &value[0..1] == "-" && &value[3..4] == "Z" => Ok(Self {
                hour: None,
                minute: Some(into_minute(&value[1..3])?),
                second: None,
                zone: Some(ZoneValue::Utc),
            }),
            // HHMM
            4 => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: Some(into_minute(&value[2..4])?),
                second: None,
                zone: None,
            }),
            // --SSZ
            5 if &value[0..2] == "--" && &value[4..5] == "Z" => Ok(Self {
                hour: None,
                minute: None,
                second: Some(into_second(&value[2..4])?),
                zone: Some(ZoneValue::Utc),
            }),
            // -MMSS
            5 if &value[0..1] == "-" => Ok(Self {
                hour: None,
                minute: Some(into_minute(&value[1..3])?),
                second: Some(into_second(&value[3..5])?),
                zone: None,
            }),
            // HHMMZ
            5 if &value[4..5] == "Z" => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: Some(into_minute(&value[2..4])?),
                second: None,
                zone: Some(ZoneValue::Utc),
            }),
            // HH+ZZ
            5 => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: None,
                second: None,
                zone: Some(ZoneValue::try_from(&value[2..5])?),
            }),
            // -MMSSZ
            6 if &value[0..1] == "-" && &value[5..6] == "Z" => Ok(Self {
                hour: None,
                minute: Some(into_minute(&value[1..3])?),
                second: Some(into_second(&value[3..5])?),
                zone: Some(ZoneValue::Utc),
            }),
            // -MM+ZZ
            6 if &value[0..1] == "-" => Ok(Self {
                hour: None,
                minute: Some(into_minute(&value[1..3])?),
                second: None,
                zone: Some(ZoneValue::try_from(&value[3..6])?),
            }),
            // HHMMSS
            6 => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: Some(into_minute(&value[2..4])?),
                second: Some(into_second(&value[4..6])?),
                zone: None,
            }),
            // --SS+ZZ
            7 if &value[0..2] == "--" => Ok(Self {
                hour: None,
                minute: None,
                second: Some(into_second(&value[2..4])?),
                zone: Some(ZoneValue::try_from(&value[4..7])?),
            }),
            // HHMMSSZ
            7 if &value[6..7] == "Z" => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: Some(into_minute(&value[2..4])?),
                second: Some(into_second(&value[4..6])?),
                zone: Some(ZoneValue::Utc),
            }),
            // HHMM+ZZ
            7 if ["+", "-"].contains(&&value[4..5]) => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: Some(into_minute(&value[2..4])?),
                second: None,
                zone: Some(ZoneValue::try_from(&value[4..7])?),
            }),
            // HH+ZZZZ
            7 => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: None,
                second: None,
                zone: Some(ZoneValue::try_from(&value[2..7])?),
            }),
            // -MM+ZZZZ
            8 if &value[0..1] == "-" => Ok(Self {
                hour: None,
                minute: Some(into_minute(&value[1..3])?),
                second: None,
                zone: Some(ZoneValue::try_from(&value[3..8])?),
            }),
            // -MMSS+ZZ
            8 if &value[0..1] == "-" => Ok(Self {
                hour: None,
                minute: Some(into_minute(&value[1..3])?),
                second: Some(into_second(&value[3..5])?),
                zone: Some(ZoneValue::try_from(&value[5..8])?),
            }),
            // --SS+ZZZZ
            9 if &value[0..2] == "--" => Ok(Self {
                hour: None,
                minute: None,
                second: Some(into_second(&value[2..4])?),
                zone: Some(ZoneValue::try_from(&value[4..9])?),
            }),
            // HHMM+ZZZZ
            9 if ["+", "-"].contains(&&value[4..5]) => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: Some(into_minute(&value[2..4])?),
                second: None,
                zone: Some(ZoneValue::try_from(&value[4..9])?),
            }),
            // HHMMSS+ZZ
            9 => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: Some(into_minute(&value[2..4])?),
                second: Some(into_second(&value[4..6])?),
                zone: Some(ZoneValue::try_from(&value[6..9])?),
            }),
            // -MMSS+ZZZZ
            10 if &value[0..1] == "-" => Ok(Self {
                hour: None,
                minute: Some(into_minute(&value[1..3])?),
                second: Some(into_second(&value[3..5])?),
                zone: Some(ZoneValue::try_from(&value[5..10])?),
            }),
            // HHMMSS+ZZZZ
            11 => Ok(Self {
                hour: Some(into_hour(&value[0..2])?),
                minute: Some(into_minute(&value[2..4])?),
                second: Some(into_second(&value[4..6])?),
                zone: Some(ZoneValue::try_from(&value[6..11])?),
            }),
            _ => Err(into_error(value)),
        }
    }
}

/// Validate that given `value` respect format for `time` values
#[must_use]
pub fn is_time_value(value: &str) -> bool {
    // time          = hour [minute [second]] [zone]
    //                /  "-"  minute [second]  [zone]
    //                /  "-"   "-"    second   [zone]
    if value.is_empty() {
        false
    } else if &value[0..1] != "-" {
        is_hour_value(&value[0..2])
            && (value.len() == 2
                || is_zone_value(&value[2..])
                || (is_minute_value(&value[2..4])
                    && (value.len() == 4
                        || is_zone_value(&value[4..])
                        || (is_second_value(&value[4..6])
                            && (value.len() == 6 || is_zone_value(&value[6..]))))))
    } else if &value[1..2] != "-" {
        is_minute_value(&value[1..3])
            && (value.len() == 3
                || is_zone_value(&value[3..])
                || (is_second_value(&value[3..5])
                    && (value.len() == 5 || is_zone_value(&value[5..]))))
    } else {
        is_second_value(&value[2..4]) && (value.len() == 4 || is_zone_value(&value[4..]))
    }
}

/// Validate that given `value` respect format for `time-notrunc` values
pub(crate) fn is_time_notrunc_value(value: &str) -> bool {
    // time-notrunc  = hour [minute [second]] [zone] ;
    let hour = value.len() > 1 && is_hour_value(&value[0..2]);
    match value.len() {
        // hour
        2 => hour,
        // hour zone
        3 => hour && is_zone_value(&value[2..]),
        // hour minute
        4 => hour && is_minute_value(&value[2..4]),
        // hour zone | hour minute zone
        5 => {
            hour && (is_zone_value(&value[2..])
                || (is_minute_value(&value[2..4]) && is_zone_value(&value[4..])))
        }
        // hour minute second
        6 => hour && is_minute_value(&value[2..4]) && is_second_value(&value[4..6]),
        // hour zone | hour minute zone | hour minute second zone
        7 => {
            hour && (is_zone_value(&value[2..])
                || (is_minute_value(&value[2..4])
                    && (is_zone_value(&value[4..])
                        || (is_second_value(&value[4..6]) && is_zone_value(&value[6..])))))
        }
        // hour minute zone | hour minute second zone
        9 => {
            hour && is_minute_value(&value[2..4])
                && (is_zone_value(&value[4..])
                    || (is_second_value(&value[4..6]) && is_zone_value(&value[6..])))
        }
        // hour minute second zone
        11 => {
            hour && is_minute_value(&value[2..4])
                && is_second_value(&value[4..6])
                && is_zone_value(&value[6..])
        }
        _ => false,
    }
}

/// Validate that given `value` respect format for `time-complete` values
pub(crate) fn is_time_complete_value(value: &str) -> bool {
    // time-complete = hour minute second [zone]
    let result = value.len() > 5
        && is_hour_value(&value[0..2])
        && is_minute_value(&value[2..4])
        && is_second_value(&value[4..6]);
    if value.len() > 6 {
        result && is_zone_value(&value[6..])
    } else {
        result
    }
}

/// Validate that given `value` respect format for `minute` values
pub(crate) fn is_minute_value(value: &str) -> bool {
    // minute = 2DIGIT  ; 00-59
    // second = 2DIGIT  ; 00-58/59/60 depending on leap second
    value.len() == 2 && value.parse::<u32>().is_ok_and(|v| v < 60)
}

/// Validate that given `value` respect format for `second` values
pub(crate) fn is_second_value(value: &str) -> bool {
    // minute = 2DIGIT  ; 00-59
    // second = 2DIGIT  ; 00-58/59/60 depending on leap second
    value.len() == 2 && value.parse::<u32>().is_ok_and(|v| v < 61)
}

/// Validate that given `value` respect format for `hour` values
pub(crate) fn is_hour_value(value: &str) -> bool {
    // hour   = 2DIGIT  ; 00-23
    value.len() == 2 && value.parse::<u32>().is_ok_and(|v| v < 24)
}
