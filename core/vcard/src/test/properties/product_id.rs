use velcro::hash_set;

use crate::ParameterType;
use crate::properties::product_id::{ProductId, validate_prodid};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text::Text;

#[test]
fn product_id_struct() {
    let product_id = ProductId::new_validated("text").unwrap();
    assert_eq!(product_id.value, Text::new_unchecked("text"));
}

#[test]
fn prodid_property() {
    validate_prodid(&make_property("PROID", Some("text"), None)).unwrap();
    validate_prodid(&make_property(
        "PROID",
        Some("text"),
        Some(vec![("VALUE", vec!["text"]), ("any", vec!["foo", "bar"])]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_prodid,
        "PRODID",
        "text",
        hash_set! {ParameterType::AltId, ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
