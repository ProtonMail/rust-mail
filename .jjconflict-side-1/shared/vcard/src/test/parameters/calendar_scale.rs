use crate::parameters::calendar_scale::{CalendarScale, is_calscale_param};
use crate::values::iana_token::IanaToken;
use crate::values::x_name::XName;

#[test]
fn calendar_scale_struct() {
    assert_eq!(
        CalendarScale::new_validated("GrEgOrIaN").unwrap(),
        CalendarScale::Gregorian
    );
    assert_eq!(
        CalendarScale::new_validated("foo").unwrap(),
        CalendarScale::IanaToken(IanaToken::new_unchecked("foo"))
    );
    assert_eq!(
        CalendarScale::new_validated("x-foo").unwrap(),
        CalendarScale::XName(XName::new_unchecked("x-foo"))
    );
}

#[test]
fn calscale_param() {
    assert!(is_calscale_param(&["gregorian".to_owned()]));
    assert!(is_calscale_param(&["foo".to_owned()]));
    assert!(is_calscale_param(&["x-bar".to_owned()]));
    assert!(!is_calscale_param(&[]));
    assert!(!is_calscale_param(&["foo".to_owned(), "x-bar".to_owned()]));
}
