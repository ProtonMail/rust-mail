use crate::ParameterType;
use crate::properties::xml::validate_xml;
use crate::test::{make_property, property_reject_parameters};
use velcro::hash_set;

#[test]
fn xml_property() {
    validate_xml(&make_property("XML", Some("text"), None)).unwrap();
    validate_xml(&make_property(
        "XML",
        Some("text"),
        Some(vec![("VALUE", vec!["text"]), ("ALTID", vec!["foo"])]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_xml,
        "XML",
        "url:url",
        hash_set! {ParameterType::Any, ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
