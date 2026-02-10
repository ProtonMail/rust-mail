use crate::parameters::sort_as::{SortAs, is_sort_as_param};

#[test]
fn sort_as_struct() {
    assert!(SortAs::new_validated(&["foo".to_owned(), "bar".to_owned()]).is_ok());
    // double quote are not valid in param-value
    assert!(SortAs::new_validated(&["foo\"bar".to_owned()]).is_err());
}

#[test]
fn sort_as_param() {
    assert!(is_sort_as_param(&["foo".to_owned()]));
    assert!(is_sort_as_param(&["foo".to_owned(), "bar".to_owned()]));
    assert!(!is_sort_as_param(&[]));
}
