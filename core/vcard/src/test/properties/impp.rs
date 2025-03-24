use velcro::hash_set;

use crate::ParameterType;
use crate::properties::impp::{Impp, validate_impp};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;

#[test]
fn impp_struct() {
    let impp = Impp::new_validated("uri:uri").unwrap();
    assert_eq!(impp.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn impp_property() {
    validate_impp(&make_property("IMPP", Some("uri:uri"), None)).unwrap();
    validate_impp(&make_property(
        "IMPP",
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
        validate_impp,
        "IMPP",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::TZ},
    );
}
