use velcro::hash_set;

use crate::properties::time_zone::{validate_tz, TimeZone, TimeZoneValue};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text::Text;
use crate::values::uri::Uri;
use crate::values::utc_offset::UTCOffset;
use crate::ParameterType;

#[test]
fn time_zone_struct() {
    let time_zone = TimeZone::new_validated("text").unwrap();
    assert_eq!(
        time_zone.value,
        TimeZoneValue::Text(Text::new_unchecked("text"))
    );
    let time_zone = TimeZone::new_validated("uri:uri").unwrap();
    assert_eq!(
        time_zone.value,
        TimeZoneValue::Uri(Uri::new_validated("uri:uri").unwrap())
    );
    let time_zone = TimeZone::new_validated("+0130").unwrap();
    assert_eq!(
        time_zone.value,
        TimeZoneValue::UtcOffset(UTCOffset::new_with_minute(1, 30))
    );
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
