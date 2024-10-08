#![allow(non_snake_case)]

use super::*;

#[test]
fn test_proton_color() {
    assert_eq!(proton_color("John Doe"), "#3C8B8C");
    assert_eq!(proton_color("Jane Doe"), "#0F735A");
    assert_eq!(proton_color("Test"), "#A839A4");
    assert_eq!(proton_color(""), "#0F735A");
}
