use velcro::hash_set;

use crate::ParameterType;
use crate::properties::language::validate_lang;
use crate::test::{make_property, property_reject_parameters};

#[test]
fn lang_property() {
    validate_lang(&make_property("LANG", Some("zh-cmn-Hans-CN"), None)).unwrap();
    validate_lang(&make_property(
        "LANG",
        Some("zh-cmn-Hans-CN"),
        Some(vec![
            ("VALUE", vec!["language-tag"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["param-value"]),
            ("TYPE", vec!["wrok", "home"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_lang,
        "LANG",
        "zh-cmn-Hans-CN",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
}
