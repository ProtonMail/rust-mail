use velcro::hash_set;

use crate::properties::fburl::{validate_fburl, FbUrl};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use crate::ParameterType;

#[test]
fn fburl_struct() {
    let fb_url = FbUrl::new_validated("uri:uri").unwrap();
    assert_eq!(fb_url.value, Uri::new("uri:uri".parse().unwrap()));
}

#[test]
fn fburl_property() {
    validate_fburl(&make_property("FBURL", Some("uri:uri"), None)).unwrap();
    validate_fburl(&make_property(
        "FBURL",
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
        validate_fburl,
        "FBURL",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
