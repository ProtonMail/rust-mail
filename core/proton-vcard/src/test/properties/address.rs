use crate::properties::address::{validate_adr, Address};
use crate::test::{make_property, property_reject_parameters};
use crate::values::component::Component;
use crate::values::list_component::ListComponent;
use crate::ParameterType;
use velcro::hash_set;

#[test]
fn address_struct() {
    let adr = Address::new_validated(
        "pobox1,pobox2",
        "ext1,ext2",
        "street1,street2",
        "locality1,locality2",
        "region1,region2",
        "code1,code2",
        "country1,country2",
    )
    .unwrap();
    assert_eq!(
        adr.post_office_box,
        ListComponent::new(&[Component::new("pobox1"), Component::new("pobox2")])
    );
    assert_eq!(
        adr.extension,
        ListComponent::new(&[Component::new("ext1"), Component::new("ext2")])
    );
    assert_eq!(
        adr.street,
        ListComponent::new(&[Component::new("street1"), Component::new("street2")])
    );
    assert_eq!(
        adr.locality,
        ListComponent::new(&[Component::new("locality1"),
            Component::new("locality2")])
    );
    assert_eq!(
        adr.region,
        ListComponent::new(&[Component::new("region1"), Component::new("region2")])
    );
    assert_eq!(
        adr.code,
        ListComponent::new(&[Component::new("code1"), Component::new("code2")])
    );
    assert_eq!(
        adr.country,
        ListComponent::new(&[Component::new("country1"),
            Component::new("country2")])
    );
}

#[test]
fn adr_property() {
    validate_adr(&make_property(
        "ADR",
        Some("pobox;ext;street;locality;region;code;country"),
        None,
    ))
    .unwrap();
    validate_adr(&make_property(
        "ADR",
        Some(r"\;;ext;street;locality;region;code;country"),
        None,
    ))
    .unwrap();
    validate_adr(&make_property(
        "ADR",
        Some("pobox;ext;street;locality;region;code;"),
        None,
    ))
    .unwrap();
    validate_adr(&make_property(
        "ADR",
        Some(r";ext;street;locality;region;code;country"),
        None,
    ))
    .unwrap();
    validate_adr(&make_property(
        "ADR",
        Some("pobox;ext;street;locality;region;code;country"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("LABEL", vec!["param-value"]),
            ("LANGUAGE", vec!["zh-cmn-Hans-CN"]),
            ("GEO", vec!["uri:uri"]),
            ("TZ", vec!["param-value"]),
            ("ALTID", vec!["param-value"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["work", "home"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    assert!(validate_adr(&make_property(
        "ADR",
        Some("pobox;ext;street;locality;region;code"),
        None
    ))
    .is_err());
    assert!(validate_adr(&make_property(
        "ADR",
        Some("pobox;ext;street;locality;region;code;country;toomany"),
        None
    ))
    .is_err());
    property_reject_parameters(
        validate_adr,
        "ADR",
        "pobox;ext;street;locality;region;code;country",
        hash_set! {ParameterType::CalScale, ParameterType::MediaType, ParameterType::SortAs},
    );
}
