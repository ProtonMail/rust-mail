use crate::ParameterType;
use crate::properties::name::validate_n;
use crate::test::{make_property, property_reject_parameters};
use velcro::hash_set;

#[test]
fn n_property() {
    validate_n(&make_property("N", Some("a,b;c;d;e;f"), None)).unwrap();
    validate_n(&make_property(
        "N",
        Some("<U+5C71><U+7530>;<U+592A><U+90CE>;;;"),
        None,
    ))
    .unwrap();
    validate_n(&make_property("N", Some(r"\;;;;;"), None)).unwrap();
    validate_n(&make_property(
        "N",
        Some("a,b;c;d;e;f"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("SORT-AS", vec!["foo", "bar"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    assert!(validate_n(&make_property("N", Some("a,b;c;d;e"), None)).is_err());
    assert!(validate_n(&make_property("N", Some("a,b;c;d;e;f;g"), None)).is_err());
    property_reject_parameters(
        validate_n,
        "N",
        "a,b;c;d;e;f",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::Type, ParameterType::TZ},
    );
}
