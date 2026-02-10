use crate::values::component::Component;
use crate::values::list_component::{ListComponent, is_list_component_value};

#[test]
fn list_component_struct() {
    let list_component =
        ListComponent::new_validated(r"\\ \,𝕯 \; \n    𝕯!+-[]~𝕯,𝕯\\ \,𝕯 \; \n     𝕯!+-[]~")
            .unwrap();
    assert_eq!(list_component.0.len(), 2);
    assert_eq!(
        list_component.0[0],
        Component::new("\\ ,𝕯 ; \n    𝕯!+-[]~𝕯")
    );
    assert_eq!(
        list_component.0[1],
        Component::new("𝕯\\ ,𝕯 ; \n     𝕯!+-[]~")
    );
}

#[test]
fn list_component_value() {
    assert!(is_list_component_value(
        r"\\ \,𝕯 \; \n    𝕯!+-[]~𝕯,𝕯\\ \,𝕯 \; \n     𝕯!+-[]~"
    ));
    assert!(is_list_component_value("a"));
    assert!(is_list_component_value("a,b"));
    assert!(is_list_component_value(""));
    assert!(!is_list_component_value("\\"));
    assert!(is_list_component_value("foo,"));
    assert!(is_list_component_value(",foo"));
    assert!(!is_list_component_value(";"));
    assert!(!is_list_component_value("\n"));
}
