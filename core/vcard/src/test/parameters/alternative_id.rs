use crate::parameters::alternative_id::{AlternativeId, is_altid_param};

#[test]
fn altid_struct() {
    assert!(AlternativeId::new_validated(r#""foo;bar""#).is_ok());
    assert!(AlternativeId::new_validated(r#""foo:bar""#).is_ok());

    // double quote are not legal inside of double quotes
    assert!(AlternativeId::new_validated(r#"foo"bar"#).is_err());
}

#[test]
fn altid_param() {
    assert!(is_altid_param(&["foo".to_owned()]));
    assert!(!is_altid_param(&["foo".to_owned(), "bar".to_owned()]));
    assert!(!is_altid_param(&[]));
}
