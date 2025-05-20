use crate::ParameterType;
use crate::properties::key::validate_key;
use crate::test::{make_property, property_reject_parameters};
use velcro::hash_set;

#[test]
fn key_property() {
    validate_key(&make_property("KEY", Some("text"), None)).unwrap();
    validate_key(&make_property("KEY", Some("uri:uri"), None)).unwrap();
    validate_key(&make_property(
        "KEY",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "2.3"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    validate_key(&make_property(
        "KEY",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "2.3"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_key,
        "KEY",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
    property_reject_parameters(
        validate_key,
        "KEY",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
