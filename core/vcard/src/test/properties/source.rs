use crate::ParameterType;
use crate::properties::source::{Source, validate_source};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::Uri;
use velcro::hash_set;

#[test]
fn source_struct() {
    let source = Source::new_validated("uri:uri").unwrap();
    assert_eq!(source.value, Uri::new_validated("uri:uri").unwrap());
}

#[test]
fn source_property() {
    validate_source(&make_property("SOURCE", Some("url:url"), None)).unwrap();
    validate_source(&make_property(
        "SOURCE",
        Some("file:/file"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("PID", vec!["1.2"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["foo"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_source,
        "SOURCE",
        "url:url",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
