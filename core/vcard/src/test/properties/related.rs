use crate::ParameterType;
use crate::properties::related::validate_related;
use crate::test::{make_property, property_reject_parameters};
use velcro::hash_set;

#[allow(clippy::too_many_lines)]
#[test]
fn related_property() {
    validate_related(&make_property("RELATED", Some("text"), None)).unwrap();
    validate_related(&make_property("RELATED", Some("uri:uri"), None)).unwrap();
    validate_related(&make_property(
        "RELATED",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["pram-value"]),
            (
                "TYPE",
                vec![
                    "work",
                    "home",
                    "contact",
                    "acquaintance",
                    "friend",
                    "met",
                    "co-worker",
                    "colleague",
                    "co-resident",
                    "neighbor",
                    "child",
                    "parent",
                    "sibling",
                    "spouse",
                    "kin",
                    "muse",
                    "crush",
                    "date",
                    "sweetheart",
                    "me",
                    "agent",
                    "emergency",
                    "iana",
                    "x-name",
                ],
            ),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    validate_related(&make_property(
        "RELATED",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["pram-value"]),
            (
                "TYPE",
                vec![
                    "work",
                    "home",
                    "contact",
                    "acquaintance",
                    "friend",
                    "met",
                    "co-worker",
                    "colleague",
                    "co-resident",
                    "neighbor",
                    "child",
                    "parent",
                    "sibling",
                    "spouse",
                    "kin",
                    "muse",
                    "crush",
                    "date",
                    "sweetheart",
                    "me",
                    "agent",
                    "emergency",
                    "iana",
                    "x-name",
                ],
            ),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    assert!(
        validate_related(&make_property(
            "RELATED",
            Some("text"),
            Some(vec![("MEDIATYPE", vec!["type/subtype"])]),
        ))
        .is_err()
    );
    assert!(
        validate_related(&make_property(
            "RELATED",
            Some("uri:uri"),
            Some(vec![
                ("VALUE", vec!["uri"]),
                ("LANGUAGE", vec!["zh-cmn-Hans-CN"])
            ]),
        ))
        .is_err()
    );
    property_reject_parameters(
        validate_related,
        "RELATED",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
    property_reject_parameters(
        validate_related,
        "RELATED",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
