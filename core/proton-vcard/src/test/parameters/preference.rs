use crate::parameters::preference::is_pref_param;

#[test]
fn pref_param() {
    assert!(is_pref_param(&["1".to_owned()]));
    assert!(!is_pref_param(&["0".to_owned()]));
    assert!(is_pref_param(&["99".to_owned()]));
    assert!(is_pref_param(&["100".to_owned()]));
    assert!(!is_pref_param(&["101".to_owned()]));
    assert!(!is_pref_param(&["1".to_owned(), "1".to_owned()]));
    assert!(!is_pref_param(&[String::new()]));
    assert!(!is_pref_param(&["foo".to_owned()]));
    assert!(!is_pref_param(&[]));
}
