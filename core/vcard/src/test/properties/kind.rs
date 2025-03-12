use velcro::hash_set;

use crate::ParameterType;
use crate::properties::kind::{Kind, KindValue, validate_kind};
use crate::test::{make_property, property_reject_parameters};
use crate::values::iana_token::IanaToken;
use crate::values::x_name::XName;

#[test]
fn kind_struct() {
    let kind = Kind::new_validated("iNdIvIdUaL").unwrap();
    assert_eq!(kind.value, KindValue::Individual);
    let kind = Kind::new_validated("gRoUp").unwrap();
    assert_eq!(kind.value, KindValue::Group);
    let kind = Kind::new_validated("oRg").unwrap();
    assert_eq!(kind.value, KindValue::Organization);
    let kind = Kind::new_validated("lOcAtIoN").unwrap();
    assert_eq!(kind.value, KindValue::Location);
    let kind = Kind::new_validated("IaNa").unwrap();
    assert_eq!(
        kind.value,
        KindValue::IanaToken(IanaToken::new_unchecked("IaNa"))
    );
    let kind = Kind::new_validated("X-NaMe").unwrap();
    assert_eq!(kind.value, KindValue::XName(XName::new_unchecked("X-NaMe")));
}

#[test]
fn kind_property() {
    validate_kind(&make_property("KIND", Some("iNdIvIdUaL"), None)).unwrap();
    validate_kind(&make_property("KIND", Some("gRoUp"), None)).unwrap();
    validate_kind(&make_property("KIND", Some("oRg"), None)).unwrap();
    validate_kind(&make_property("KIND", Some("lOcAtIoN"), None)).unwrap();
    validate_kind(&make_property("KIND", Some("iAnA"), None)).unwrap();
    validate_kind(&make_property(
        "KIND",
        Some("x-name"),
        Some(vec![("VALUE", vec!["text"]), ("any", vec!["foo", "bar"])]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_kind,
        "KIND",
        "org",
        hash_set! {ParameterType::AltId, ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::Pid, ParameterType::Pref, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
