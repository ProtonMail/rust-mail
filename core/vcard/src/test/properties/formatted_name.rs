use crate::properties::formatted_name::{validate_fn, FormattedName};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text::Text;
use crate::ParameterType;
use velcro::hash_set;

#[test]
fn formatted_name_struct() {
    let formatted_name = FormattedName::new_validated("text").unwrap();
    assert_eq!(formatted_name.value, Text::new_unchecked("text"));
}

#[test]
fn fn_property() {
    validate_fn(&make_property("FN", Some("text"), None)).unwrap();
    validate_fn(&make_property(
        "FN",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("TYPE", vec!["work", "home"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_fn,
        "FN",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
}
