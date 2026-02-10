use crate::values::date_and_or_time::{DateAndOrTimeValue, is_date_and_or_time_value};

#[test]
fn date_and_or_time_struct() {
    let value = DateAndOrTimeValue::new_validated("T09").unwrap();
    assert_eq!(value.0.year, None);
    assert_eq!(value.0.month, None);
    assert_eq!(value.0.day, None);
    assert_eq!(value.0.hour, Some(9));
    assert_eq!(value.0.minute, None);
    assert_eq!(value.0.second, None);
    assert_eq!(value.0.zone, None);
    let value = DateAndOrTimeValue::new_validated("2014").unwrap();
    assert_eq!(value.0.year, Some(2014));
    assert_eq!(value.0.month, None);
    assert_eq!(value.0.day, None);
    assert_eq!(value.0.hour, None);
    assert_eq!(value.0.minute, None);
    assert_eq!(value.0.second, None);
    assert_eq!(value.0.zone, None);
    let value = DateAndOrTimeValue::new_validated("20140614T09").unwrap();
    assert_eq!(value.0.year, Some(2014));
    assert_eq!(value.0.month, Some(6));
    assert_eq!(value.0.day, Some(14));
    assert_eq!(value.0.hour, Some(9));
    assert_eq!(value.0.minute, None);
    assert_eq!(value.0.second, None);
    assert_eq!(value.0.zone, None);
}

#[test]
fn date_and_or_time_value() {
    assert!(is_date_and_or_time_value("T09"));
    assert!(is_date_and_or_time_value("2014"));
    assert!(is_date_and_or_time_value("20140614T09"));
    assert!(!is_date_and_or_time_value(""));
    assert!(!is_date_and_or_time_value("foo"));
}
