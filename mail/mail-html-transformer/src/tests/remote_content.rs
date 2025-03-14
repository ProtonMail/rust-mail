#![allow(non_snake_case)]

use crate::Transformer;

const TEST_DOCUMENT: &str = include_str!("test_document.html");

#[test]
fn disable_remote_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let (remote, embedded) = transformer.disable_content(true, false);
    insta::assert_snapshot!(transformer.to_string());

    assert_eq!(remote, 8);
    assert_eq!(embedded, 0, "Disabled embedded content!?");
}

#[test]
fn disable_embedded_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let (remote, embedded) = transformer.disable_content(false, true);
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(remote, 0, "Disabled remote content!?");
    assert_eq!(embedded, 9);
}

#[test]
fn disable_all_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let (remote, embedded) = transformer.disable_content(true, true);
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(remote, 8);
    assert_eq!(embedded, 9);
}
