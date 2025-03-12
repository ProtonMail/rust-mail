use crate::PropertyKind;
use crate::parameters::type_generic::is_type_param;
use crate::parameters::type_tel::TelType;
use crate::values::iana_token::IanaToken;
use crate::values::x_name::XName;

#[test]
fn tel_type_enum() {
    assert_eq!(TelType::new_validated("HoMe").unwrap(), TelType::Home);
    assert_eq!(TelType::new_validated("WoRk").unwrap(), TelType::Work);
    assert_eq!(TelType::new_validated("TeXt").unwrap(), TelType::Text);
    assert_eq!(TelType::new_validated("VoIcE").unwrap(), TelType::Voice);
    assert_eq!(TelType::new_validated("FaX").unwrap(), TelType::Fax);
    assert_eq!(TelType::new_validated("CeLl").unwrap(), TelType::Cell);
    assert_eq!(TelType::new_validated("ViDeO").unwrap(), TelType::Video);
    assert_eq!(TelType::new_validated("PaGeR").unwrap(), TelType::Pager);
    assert_eq!(
        TelType::new_validated("TeXtPhOnE").unwrap(),
        TelType::TextPhone
    );
    assert_eq!(
        TelType::new_validated("X-name").unwrap(),
        TelType::XName(XName::new_unchecked("X-name"))
    );
    assert_eq!(
        TelType::new_validated("iana-token").unwrap(),
        TelType::IanaToken(IanaToken::new_unchecked("iana-token"))
    );
}

#[test]
fn type_param() {
    assert!(is_type_param(
        &PropertyKind::Tel,
        &[
            "text".to_owned(),
            "voice".to_owned(),
            "fax".to_owned(),
            "cell".to_owned(),
            "video".to_owned(),
            "pager".to_owned(),
            "textphone".to_owned(),
            "work".to_owned(),
            "home".to_owned(),
            "iana-token".to_owned(),
            "x-name".to_owned(),
        ]
    ));
}
