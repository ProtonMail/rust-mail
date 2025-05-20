use velcro::hash_set;

use crate::ParameterType;
use crate::properties::note::validate_note;
use crate::test::{make_property, property_reject_parameters};

#[test]
fn note_property() {
    validate_note(&make_property("NOTE", Some("text"), None)).unwrap();
    validate_note(&make_property(
        "NOTE",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["home", "work"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_note,
        "NOTE",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
}
