use velcro::hash_set;

use crate::properties::revision::{validate_rev, Revision};
use crate::test::{make_property, property_reject_parameters};
use crate::values::timestamp::Timestamp;
use crate::ParameterType;

#[test]
fn revision_struct() {
    let revision = Revision::new_validated("99991231T235959+2359").unwrap();
    assert_eq!(
        revision.value,
        Timestamp::new_validated("99991231T235959+2359").unwrap()
    );
}

#[test]
fn rev_property() {
    validate_rev(&make_property("REV", Some("99991231T235959+2359"), None)).unwrap();
    validate_rev(&make_property(
        "REV",
        Some("99991231T235959+2359"),
        Some(vec![
            ("VALUE", vec!["timestamp"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_rev,
        "REV",
        "99991231T235959+2359",
        hash_set! {ParameterType::AltId, ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
