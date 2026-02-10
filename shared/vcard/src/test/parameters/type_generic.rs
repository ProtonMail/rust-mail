use crate::PropertyKind;
use crate::parameters::type_generic::{GenericType, is_type_param};
use crate::values::iana_token::IanaToken;
use crate::values::x_name::XName;

#[test]
fn generic_type_enum() {
    assert_eq!(
        GenericType::new_validated("HoMe").unwrap(),
        GenericType::Home
    );
    assert_eq!(
        GenericType::new_validated("WoRk").unwrap(),
        GenericType::Work
    );
    assert_eq!(
        GenericType::new_validated("x-foo").unwrap(),
        GenericType::XName(XName::new_unchecked("x-foo"))
    );
    assert_eq!(
        GenericType::new_validated("foo").unwrap(),
        GenericType::IanaToken(IanaToken::new_unchecked("foo"))
    );
}

#[test]
fn type_param() {
    assert!(is_type_param(
        &PropertyKind::Kind,
        &[
            "work".to_owned(),
            "home".to_owned(),
            "iana-token".to_owned(),
            "x-name".to_owned(),
        ]
    ));
}
