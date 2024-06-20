use velcro::hash_set;

use crate::properties::role::{validate_role, Role};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text::Text;
use crate::ParameterType;

#[test]
fn role_struct() {
    let role = Role::new_validated("text").unwrap();
    assert_eq!(role.value, Text::new_unchecked("text"));
}

#[test]
fn role_property() {
    validate_role(&make_property("ROLE", Some("text"), None)).unwrap();
    validate_role(&make_property(
        "ROLE",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_role,
        "ROLE",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
}
