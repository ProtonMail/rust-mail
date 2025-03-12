use crate::properties::logo::{validate_logo, Logo};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use crate::ParameterType;
use velcro::hash_set;

#[test]
fn logo_struct() {
    let logo = Logo::new_validated("uri:uri").unwrap();
    assert_eq!(logo.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn logo_property() {
    validate_logo(&make_property("LOGO", Some("uri:uri"), None)).unwrap();
    validate_logo(&make_property(
        "LOGO",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["home", "work"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_logo,
        "LOGO",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::SortAs, ParameterType::TZ},
    );
}
