mod address;
mod anniversary;
mod birthday;
mod calendar_uri;
mod calendar_user_address;
mod categories;
mod client_pid_map;
mod email;
mod fburl;
mod formatted_name;
mod gender;
mod geo;
mod impp;
mod key;
mod kind;
mod language;
mod logo;
mod member;
mod name;
mod nickname;
mod note;
mod organization;
mod photo;
mod product_id;
mod related;
mod revision;
mod role;
mod sound;
mod source;
mod telephone;
mod time_zone;
mod title;
mod uid;
mod url;
mod xml;

use crate::properties::begin::validate_begin;
use crate::properties::end::validate_end;
use crate::properties::version::validate_version;
use crate::properties::xtended::Xtended;
use crate::test::{make_property, property_reject_parameters};
use crate::values::x_name::XName;
use crate::ParameterType;
use velcro::hash_set;

#[test]
fn begin_property() {
    validate_begin(&make_property("BEGIN", Some("VCARD"), None)).unwrap();
    assert!(validate_begin(&make_property("BEGIN", Some("OTHER"), None)).is_err());
}

#[test]
fn end_property() {
    validate_end(&make_property("END", Some("VCARD"), None)).unwrap();
    assert!(validate_end(&make_property("END", Some("OTHER"), None)).is_err());
}

#[test]
fn version_property() {
    validate_version(&make_property("VERSION", Some("4.0"), None)).unwrap();
    validate_version(&make_property(
        "VERSION",
        Some("4.0"),
        Some(vec![("VALUE", vec!["text"]), ("any", vec!["foo", "bar"])]),
    ))
    .unwrap();
    assert!(validate_version(&make_property("VERSION", Some("foo"), None)).is_err());
    property_reject_parameters(
        validate_version,
        "VERSION",
        "4.0",
        hash_set! {ParameterType::AltId, ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}

#[test]
fn xtended_struct() {
    let xtended = Xtended::new_validated("foo", Some("bar".to_owned())).unwrap();
    assert_eq!(xtended.name, XName::new_unchecked("X-foo"));
    assert_eq!(xtended.value.unwrap(), "bar");
}
