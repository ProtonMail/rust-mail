use velcro::hash_set;

use crate::properties::title::{validate_title, Title};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text::Text;
use crate::ParameterType;

#[test]
fn title_struct() {
    let title = Title::new_validated("text").unwrap();
    assert_eq!(title.value, Text::new_unchecked("text"));
}

#[test]
fn title_property() {
    validate_title(&make_property("TITLE", Some("text"), None)).unwrap();
    validate_title(&make_property(
        "TITLE",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["pram-value"]),
            ("TYPE", vec!["home", "work"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_title,
        "TITLE",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
}
