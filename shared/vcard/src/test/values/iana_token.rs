use crate::values::iana_token::{IanaToken, is_iana_token_value};

#[test]
fn iana_token_struct() {
    let value = "0123456789-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let iana_token = IanaToken::new_validated(value).unwrap();
    assert_eq!(iana_token.0, value);
}

#[test]
fn iana_token_value() {
    assert!(is_iana_token_value(
        "0123456789-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
    ));
    assert!(!is_iana_token_value("𝕯"));
    assert!(!is_iana_token_value(""));
}
