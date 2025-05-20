use std::fmt::Debug;

use crate::values::date::{DateValue, is_date_noreduc_value};
use crate::values::time::{TimeValue, is_time_notrunc_value};
use crate::values::zone::ZoneValue;

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateTimeValue {
    pub year: Option<u16>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub second: Option<u8>,
    pub zone: Option<ZoneValue>,
}

// TODO? split in many many enum for all possible used combination
// TODO? and/or add validation depending on valid combination
// All patterns:
//    YYYY                     ; 4 ; date
//    YYYYMMDD                 ; 8 ; date | date-noreduc | date-complete
//    YYYY-MM                  ; 7 ; date
//    --MM                     ; 4 ; date
//    --MMDD                   ; 6 ; date | date-noreduc
//    ---DD                    ; 5 ; date | date-noreduc
//                             ; all time and datetime have optionally a zone (Z = 1 | 3 | 5)
//    HH                       ;  2 +Z ; time | time-notrunc
//    HHmm                     ;  4 +Z ; time | time-notrunc
//    HHmmss                   ;  6 +Z ; time | time-notrunc | time-complete
//    -mm                      ;  3 +Z ; time
//    -mmss                    ;  5 +Z ; time
//    --ss                     ;  4 +Z ; time
//    YYYYMMDDTHH              ; 11 +Z ; datetime
//    YYYYMMDDTHHmm            ; 13 +Z ; datetime
//    YYYYMMDDTHHmmss          ; 15 +Z ; datetime | timestamp
//    --MMDDTHH                ;  9 +Z ; datetime
//    --MMDDTHHmm              ; 11 +Z ; datetime
//    --MMDDTHHmmss            ; 13 +Z ; datetime
//    ---DDTHH                 ;  8 +Z ; datetime
//    ---DDTHHmm               ; 10 +Z ; datetime
//    ---DDTHHmmss             ; 12 +Z ; datetime

impl From<TimeValue> for DateTimeValue {
    fn from(value: TimeValue) -> Self {
        Self {
            year: None,
            month: None,
            day: None,
            hour: value.hour,
            minute: value.minute,
            second: value.second,
            zone: value.zone,
        }
    }
}

impl From<DateValue> for DateTimeValue {
    fn from(value: DateValue) -> Self {
        Self {
            year: value.year,
            month: value.month,
            day: value.day,
            hour: None,
            minute: None,
            second: None,
            zone: None,
        }
    }
}

impl DateTimeValue {
    pub(crate) fn from_date_and_time(date: &DateValue, time: &TimeValue) -> Self {
        Self {
            year: date.year,
            month: date.month,
            day: date.day,
            hour: time.hour,
            minute: time.minute,
            second: time.second,
            zone: time.zone.clone(),
        }
    }
}

/// Validate that given `value` respect format for `date-time` values
#[must_use]
pub fn is_date_time_value(value: &str) -> bool {
    // date-time = date-noreduc  time-designator time-notrunc
    // time-designator = %x54 ; uppercase "T"
    let values: Vec<_> = value.split('T').collect();
    values.len() == 2 && is_date_noreduc_value(values[0]) && is_time_notrunc_value(values[1])
}
