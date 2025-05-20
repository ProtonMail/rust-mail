mod alternative_id;
mod any;
mod calendar_scale;
mod geo_localisation;
mod label;
mod mediatype;
mod pid;
mod preference;
mod sort_as;
mod time_zone;
mod type_generic;
mod type_related;
mod type_tel;

use crate::parameters::have_no_param;

use crate::ValueType;
use crate::parameters::value::is_value_param;

#[test]
fn value_param() {
    assert!(is_value_param(
        &["date-and-or-time".to_owned()],
        ValueType::DateAndOrTime
    ));
    assert!(is_value_param(
        &["language-tag".to_owned()],
        ValueType::LanguageTag
    ));
    assert!(is_value_param(&["text".to_owned()], ValueType::Text));
    assert!(is_value_param(&["uri".to_owned()], ValueType::Uri));
    assert!(is_value_param(
        &["utc-offset".to_owned()],
        ValueType::UTCOffset
    ));
    assert!(!is_value_param(&["unexpected".to_owned()], ValueType::Text));
    assert!(!is_value_param(
        &["text".to_owned(), "text".to_owned()],
        ValueType::Text
    ));
    assert!(!is_value_param(&[], ValueType::Uri));
}

#[test]
fn no_param() {
    assert!(have_no_param(None));
    assert!(have_no_param(Some(&[])));
    assert!(!have_no_param(Some(&[("foo".to_owned(), vec![])])));
}
