use crate::ParameterType;
use crate::properties::anniversary::validate_anniversary;
use crate::test::{make_property, property_reject_parameters};
use velcro::hash_set;

#[test]
fn anniversary_property() {
    validate_anniversary(&make_property("ANNIVERSARY", Some("T01"), None)).unwrap();
    validate_anniversary(&make_property(
        "ANNIVERSARY",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    validate_anniversary(&make_property(
        "ANNIVERSARY",
        Some("T01"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    validate_anniversary(&make_property(
        "ANNIVERSARY",
        Some("T01"),
        Some(vec![
            ("VALUE", vec!["date-and-or-time"]),
            ("ALTID", vec!["param-value"]),
            ("CALSCALE", vec!["gregorian"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_anniversary,
        "ANNIVERSARY",
        "T01",
        hash_set! {ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
    property_reject_parameters(
        validate_anniversary,
        "ANNIVERSARY",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
