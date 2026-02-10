use crate::values::timestamp::{Timestamp, is_timestamp_value};
use chrono::{Datelike, FixedOffset, Timelike};

#[test]
fn timestamp_struct() {
    let timestamp = Timestamp::new_validated("19961022T140000-0500").unwrap();
    assert_eq!(timestamp.0.year(), 1996);
    assert_eq!(timestamp.0.month(), 10);
    assert_eq!(timestamp.0.day(), 22);
    assert_eq!(timestamp.0.hour(), 14);
    assert_eq!(timestamp.0.minute(), 0);
    assert_eq!(timestamp.0.second(), 0);
    assert_eq!(
        timestamp.0.timezone(),
        FixedOffset::east_opt(-5 * 60 * 60).unwrap()
    );
}

#[test]
fn timestamp_value() {
    assert!(is_timestamp_value("99991231T235959+2359"));
    assert!(is_timestamp_value("99991231T235959"));
    assert!(!is_timestamp_value(""));
}
