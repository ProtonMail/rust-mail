use crate::properties::xml::{validate_xml, Xml};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text::Text;
use crate::ParameterType;
use velcro::hash_set;

#[test]
fn xml_struct() {
    let xml = Xml::new_validated("text").unwrap();
    assert_eq!(xml.value, Text::new_unchecked("text"));
}

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
