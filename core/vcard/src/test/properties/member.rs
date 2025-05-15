use crate::ParameterType;
use crate::properties::member::{Member, validate_member};
use crate::test::{make_property, property_reject_parameters};
use crate::values::uri::MaybeUri;
use velcro::hash_set;

#[test]
fn member_struct() {
    let member = Member::new("uri:uri".into());
    assert_eq!(member.value, MaybeUri::Text("uri:uri".into()));
}

#[test]
fn member_property() {
    validate_member(&make_property("MEMBER", Some("uri:uri"), None)).unwrap();
    validate_member(&make_property(
        "MEMBER",
        Some("uri:uri"),
        Some(vec![
            ("VALUE", vec!["uri"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("ALTID", vec!["param-value"]),
            ("MEDIATYPE", vec!["type/subtype"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_member,
        "MEMBER",
        "uri:uri",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::SortAs, ParameterType::Type, ParameterType::TZ},
    );
}
