use crate::values::param_value::{ParamValue, is_param_value};

#[test]
fn param_value_struct() {
    let param_value = ParamValue::new_validated(" \t ! #9<~𝕯").unwrap();
    assert_eq!(param_value.value, " \t ! #9<~𝕯");
    let param_value = ParamValue::new_validated("\" \t ! #:;~𝕯\"").unwrap();
    assert_eq!(param_value.value, "\" \t ! #:;~𝕯\"");
    assert!(ParamValue::new_validated("foo\"bar").is_err());
}

#[test]
fn param_value() {
    assert!(is_param_value(" \t ! #9<~𝕯"));
    assert!(is_param_value("\" \t ! #:;~𝕯\""));
    assert!(!is_param_value("\""));
}
