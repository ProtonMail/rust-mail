use crate::parameters::label::{Label, is_label_param};

#[test]
fn label_struct() {
    assert!(Label::new_validated("foo").is_ok());

    // double quote are invalid in param-value
    assert!(Label::new_validated("foo\"bar").is_err());
}

#[test]
fn label_param() {
    assert!(is_label_param(&["foo".to_owned()]));
    assert!(!is_label_param(&[]));
    assert!(!is_label_param(&["foo".to_owned(), "bar".to_owned()]));
}
