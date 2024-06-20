use velcro::hash_set;

use crate::properties::calendar_user_address::{validate_caladruri, CalendarUserAddress};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use crate::ParameterType;

#[test]
fn calendar_user_address() {
    let caladruri = CalendarUserAddress::new_validated("uri:uri").unwrap();
    assert_eq!(caladruri.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn caladruri_property() {
    validate_caladruri(&make_property("CALADRURI", Some("uri:uri"), None)).unwrap();
    validate_caladruri(&make_property(
        "CALADRURI",
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
        validate_caladruri,
        "CALADRURI",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
