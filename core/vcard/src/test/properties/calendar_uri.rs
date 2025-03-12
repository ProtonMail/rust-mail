use crate::properties::calendar_uri::{validate_caluri, CalendarAddress};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use crate::ParameterType;
use velcro::hash_set;

#[test]
fn calendar_uri_struct() {
    let caluri = CalendarAddress::new_validated("uri:uri").unwrap();
    assert_eq!(caluri.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn caluri_property() {
    validate_caluri(&make_property("CALURI", Some("uri:uri"), None)).unwrap();
    validate_caluri(&make_property(
        "CALURI",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["home", "work"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_caluri,
        "CALURI",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
