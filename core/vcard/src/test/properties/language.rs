use velcro::hash_set;

use crate::properties::language::{validate_lang, Language};
use crate::test::{make_property, property_reject_parameters};
use crate::values::language_tag::LanguageTag;
use crate::ParameterType;

#[test]
fn language_struct() {
    let language = Language::new_validated("zh-cmn-Hans-CN").unwrap();
    assert_eq!(
        language.value,
        LanguageTag::new_validated("zh-cmn-Hans-CN").unwrap()
    );
}

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
