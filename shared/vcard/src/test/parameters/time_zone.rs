use crate::parameters::time_zone::{TimeZone, is_tz_param};
use crate::values::param_value::ParamValue;
use crate::values::uri::Uri;

#[test]
fn time_zone_enum() {
    assert_eq!(
        TimeZone::new_validated("foo").unwrap(),
        TimeZone::ParamValue(ParamValue::new_unchecked("foo"))
    );
    assert_eq!(
        TimeZone::new_validated("uri:uri").unwrap(),
        TimeZone::Uri(Uri::new("uri:uri".parse().unwrap()))
    );
    // double quote are not valid in param-value
    assert!(TimeZone::new_validated("foo\"bar").is_err());
}

#[test]
fn tz_param() {
    assert!(is_tz_param(&[
        "ftp://ftp.is.co.za/rfc/rfc1808.txt".to_owned()
    ]));
    assert!(is_tz_param(&["foo".to_owned()]));
    assert!(!is_tz_param(&[]));
    assert!(!is_tz_param(&[
        "ftp://ftp.is.co.za/rfc/rfc1808.txt".to_owned(),
        "foo".to_owned()
    ]));
}
