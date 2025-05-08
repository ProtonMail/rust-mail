use velcro::hash_set;

use crate::ParameterType;
use crate::properties::time_zone::{TimeZoneValue, validate_tz};
use crate::test::{make_property, property_reject_parameters};

#[test]
fn time_zone_struct() {
    let tz_text = TimeZoneValue::from("text");
    let tz_uri = TimeZoneValue::from("uri:uri");
    let tz_tz = TimeZoneValue::from("+0130");

    assert!(matches!(tz_text, TimeZoneValue::Text(_)));
    assert!(matches!(tz_uri, TimeZoneValue::Uri(_)));
    assert!(matches!(tz_tz, TimeZoneValue::UtcOffset(_)));
}

#[test]
fn tz_property() {
    validate_tz(&make_property("TZ", Some("text"), None)).unwrap();
    validate_tz(&make_property("TZ", Some("uri:uri"), None)).unwrap();
    validate_tz(&make_property("TZ", Some("+01"), None)).unwrap();
    validate_tz(&make_property(
        "TZ",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_tz,
        "TZ",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
    validate_tz(&make_property(
        "TZ",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_tz,
        "TZ",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
    validate_tz(&make_property(
        "TZ",
        Some("+01"),
        Some(vec![
            ("VALUE", vec!["utc-offset"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_tz,
        "TZ",
        "+01",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
