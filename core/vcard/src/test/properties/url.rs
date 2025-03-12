use velcro::hash_set;

use crate::properties::url::{validate_url, VcardUrl};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use crate::ParameterType;

#[test]
fn url_struct() {
    let url = VcardUrl::new_validated("uri:uri").unwrap();
    assert_eq!(url.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn url_property() {
    validate_url(&make_property("URL", Some("uri:uri"), None)).unwrap();
    validate_url(&make_property(
        "URL",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_url,
        "URL",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
