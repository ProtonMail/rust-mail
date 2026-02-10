use crate::values::date::{DateValue, is_date_value};

#[test]
fn date_struct() {
    let date = DateValue::new_validated("2014").unwrap();
    assert_eq!(date.year, Some(2014));
    assert_eq!(date.month, None);
    assert_eq!(date.day, None);
    let date = DateValue::new_validated("20140614").unwrap();
    assert_eq!(date.year, Some(2014));
    assert_eq!(date.month, Some(6));
    assert_eq!(date.day, Some(14));
    let date = DateValue::new_validated("2014-06").unwrap();
    assert_eq!(date.year, Some(2014));
    assert_eq!(date.month, Some(6));
    assert_eq!(date.day, None);
    let date = DateValue::new_validated("--06").unwrap();
    assert_eq!(date.year, None);
    assert_eq!(date.month, Some(6));
    assert_eq!(date.day, None);
    let date = DateValue::new_validated("--0614").unwrap();
    assert_eq!(date.year, None);
    assert_eq!(date.month, Some(6));
    assert_eq!(date.day, Some(14));
    let date = DateValue::new_validated("---14").unwrap();
    assert_eq!(date.year, None);
    assert_eq!(date.month, None);
    assert_eq!(date.day, Some(14));
}

#[test]
fn date_value() {
    assert!(is_date_value("2014"));
    assert!(is_date_value("20140614"));
    assert!(is_date_value("2014-06"));
    assert!(is_date_value("--06"));
    assert!(is_date_value("--0614"));
    assert!(is_date_value("---14"));
    assert!(!is_date_value(""));
    assert!(!is_date_value("foo"));
}
