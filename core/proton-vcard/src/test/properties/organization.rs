use velcro::hash_set;

use crate::properties::organization::{validate_org, Organization};
use crate::test::{make_property, property_reject_parameters};
use crate::values::component::Component;
use crate::ParameterType;

#[test]
fn organization_struct() {
    let organization = Organization::new_validated("a;b").unwrap();
    assert_eq!(organization.values.len(), 2);
    assert_eq!(organization.values[0], Component::new("a"));
    assert_eq!(organization.values[1], Component::new("b"));
}

#[test]
fn org_property() {
    validate_org(&make_property("ORG", Some("text"), None)).unwrap();
    validate_org(&make_property(
        "ORG",
        Some("text"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("SORT-AS", vec!["foo", "bar"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["param-value"]),
            ("TYPE", vec!["home", "work"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_org,
        "ORG",
        "text",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::MediaType, ParameterType::TZ},
    );
}
