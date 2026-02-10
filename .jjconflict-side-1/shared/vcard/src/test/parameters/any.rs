use crate::parameters::any::{Any, is_any_param};

#[test]
fn any_struct() {
    // invalid name
    assert!(Any::new_validated("𝕯", &["foo".to_owned()]).is_err());

    // no empty values
    assert!(Any::new_validated("foo", &[]).is_err());

    // invalid param-value
    assert!(Any::new_validated("foo", &["bar\"baz".to_owned()]).is_err());
}

#[test]
fn any_param() {
    assert!(is_any_param("foo", &["bar".to_owned(), "caz".to_owned()]));
    assert!(is_any_param("x-foo", &["bar".to_owned()]));
    assert!(!is_any_param("𝕯", &["bar".to_owned()]));
    assert!(!is_any_param("foo", &[]));
}
