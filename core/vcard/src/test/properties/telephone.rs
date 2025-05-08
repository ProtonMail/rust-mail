use crate::ParameterType;
use crate::properties::telephone::{Telephone, TelephoneValue, validate_tel};
use crate::test::{make_property, property_reject_parameters};
use velcro::hash_set;

#[test]
fn telephone_struct() {
    let telephone = Telephone::new("text".to_string());
    assert_eq!(telephone.value, TelephoneValue::Text("text".to_string()));

    let telephone = Telephone::new("uri:uri".to_string());
    assert_eq!(
        telephone.value,
        TelephoneValue::Uri("uri:uri".parse().unwrap())
    );
}

#[test]
fn tel_property() {
    validate_tel(&make_property("TEL", Some("text"), None)).unwrap();
    validate_tel(&make_property("TEL", Some("uri:uri"), None)).unwrap();
    validate_tel(&make_property(
        "TEL",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            (
                "TYPE",
                vec![
                    "work",
                    "home",
                    "text",
                    "voice",
                    "fax",
                    "cell",
                    "video",
                    "pager",
                    "textphone",
                    "iana",
                    "x-name",
                ],
            ),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    validate_tel(&make_property(
        "TEL",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            (
                "TYPE",
                vec![
                    "work",
                    "home",
                    "text",
                    "voice",
                    "fax",
                    "cell",
                    "video",
                    "pager",
                    "textphone",
                    "iana",
                    "x-name",
                ],
            ),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_tel,
        "TEL",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
    property_reject_parameters(
        validate_tel,
        "TEL",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
