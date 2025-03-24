use crate::ParameterType;
use crate::properties::name::{Name, validate_n};
use crate::test::{make_property, property_reject_parameters};
use crate::values::list_component::ListComponent;
use velcro::hash_set;

#[test]
fn name_struct() {
    let name = Name::new_validated("a,b", "c,d", "e,f", "g,h", "i,j").unwrap();
    assert_eq!(name.last, ListComponent::new_validated("a,b").unwrap());
    assert_eq!(name.first, ListComponent::new_validated("c,d").unwrap());
    assert_eq!(
        name.additional,
        ListComponent::new_validated("e,f").unwrap()
    );
    assert_eq!(name.prefix, ListComponent::new_validated("g,h").unwrap());
    assert_eq!(name.suffix, ListComponent::new_validated("i,j").unwrap());
}

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
