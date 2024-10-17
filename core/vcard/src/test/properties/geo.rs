use velcro::hash_set;

use crate::properties::geo::{validate_geo, Geo};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use crate::ParameterType;

#[test]
fn geo_struct() {
    let geo = Geo::new_validated("uri:uri").unwrap();
    assert_eq!(geo.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn geo_property() {
    validate_geo(&make_property("GEO", Some("uri:uri"), None)).unwrap();
    validate_geo(&make_property(
        "GEO",
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
        validate_geo,
        "GEO",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
