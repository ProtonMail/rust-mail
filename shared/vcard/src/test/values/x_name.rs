use crate::values::x_name::{XName, is_x_name_value};

#[test]
fn x_name_struct() {
    let value = "x-0123456789-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let x_name = XName::new_validated(value).unwrap();
    assert_eq!(x_name.0, value);
}

#[test]
fn x_name_value() {
    assert!(is_x_name_value(
        "x-0123456789-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
    ));
    assert!(!is_x_name_value("x-𝕯"));
    assert!(!is_x_name_value("09-azAZ"));
    assert!(!is_x_name_value(""));
}
