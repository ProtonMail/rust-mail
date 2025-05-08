use crate::ParameterType;
use crate::properties::address::validate_adr;
use crate::test::{make_property, property_reject_parameters};
use velcro::hash_set;

#[test]
fn adr_property() {
    validate_adr(&make_property(
        "ADR",
        Some("pobox;ext;street;locality;region;code;country"),
        None,
    ))
    .unwrap();
    validate_adr(&make_property(
        "ADR",
        Some(r"\;;ext;street;locality;region;code;country"),
        None,
    ))
    .unwrap();
    validate_adr(&make_property(
        "ADR",
        Some("pobox;ext;street;locality;region;code;"),
        None,
    ))
    .unwrap();
    validate_adr(&make_property(
        "ADR",
        Some(r";ext;street;locality;region;code;country"),
        None,
    ))
    .unwrap();
    validate_adr(&make_property(
        "ADR",
        Some("pobox;ext;street;locality;region;code;country"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("LABEL", vec!["param-value"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("GEO", vec!["uri:uri"]),
            ("TZ", vec!["param-value"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    assert!(
        validate_adr(&make_property(
            "ADR",
            Some("pobox;ext;street;locality;region;code"),
            None
        ))
        .is_err()
    );
    assert!(
        validate_adr(&make_property(
            "ADR",
            Some("pobox;ext;street;locality;region;code;country;toomany"),
            None
        ))
        .is_err()
    );
    property_reject_parameters(
        validate_adr,
        "ADR",
        "pobox;ext;street;locality;region;code;country",
        hash_set! {ParameterType::CalScale, ParameterType::MediaType, ParameterType::SortAs},
    );
}
