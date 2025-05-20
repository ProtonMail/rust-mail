use velcro::hash_set;

use crate::ParameterType;
use crate::properties::uid::validate_uid;
use crate::test::{make_property, property_reject_parameters};

#[test]
fn uid_property() {
    validate_uid(&make_property("UID", Some("text"), None)).unwrap();
    validate_uid(&make_property("UID", Some("uri:uri"), None)).unwrap();
    validate_uid(&make_property(
        "UID",
        Some("text"),
        Some(vec![("VALUE", vec!["text"]), ("any", vec!["foo", "bar"])]),
    ))
    .unwrap();
    validate_uid(&make_property(
        "UID",
        Some("uri:uri"),
        Some(vec![("VALUE", vec!["uri"]), ("any", vec!["foo", "bar"])]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_uid,
        "UID",
        "text",
        hash_set! {ParameterType::AltId, ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
    property_reject_parameters(
        validate_uid,
        "UID",
        "uri:uri",
        hash_set! {ParameterType::AltId, ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
