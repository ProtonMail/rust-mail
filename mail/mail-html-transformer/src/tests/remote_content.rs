use crate::Transformer;

const TEST_DOCUMENT: &str = include_str!("html/smoke.html");

#[test]
fn disable_remote_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let (remote, _) = transformer.disable_content(true, false);
    insta::assert_snapshot!(transformer.to_string());

    assert_eq!(remote, 8);
}

#[test]
fn disable_embedded_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let (_, embedded) = transformer.disable_content(false, true);
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(embedded, 12);
}

#[test]
fn disable_all_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let (remote, embedded) = transformer.disable_content(true, true);
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(remote, 8);
    assert_eq!(embedded, 9);
}

#[test]
fn disable_all_elements_uri_test() {
    let html = include_str!("../../tests/htmls/strip_uri_elements.html");
    let mut transformer = Transformer::new(html);
    let (_, _) = transformer.disable_content(true, true);
    insta::assert_snapshot!(transformer.to_string());
}
