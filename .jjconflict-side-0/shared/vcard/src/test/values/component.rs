use crate::values::component::{Component, is_component_value};

#[test]
fn component_struct() {
    let value = Component::new_from_vcard(r"\\ \, \; \n     𝕯!+-[]~").unwrap();
    assert_eq!(value, Component::new("\\ , ; \n     𝕯!+-[]~"));
}

#[test]
fn component_value() {
    assert!(is_component_value(r"\\ \, \; \n     𝕯!+-[]~"));
    assert!(is_component_value("a"));
    assert!(is_component_value(""));
    assert!(!is_component_value("foo,"));
    assert!(!is_component_value(";"));
    assert!(!is_component_value("\n"));
}
