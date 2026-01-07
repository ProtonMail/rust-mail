use crate::Transformer;

const TEST_DOCUMENT: &str = include_str!("html/smoke.html");

#[test]
fn disable_remote_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let disabled = transformer.disable_content(true, false);
    insta::assert_snapshot!(transformer.to_string());

    assert_eq!(disabled.remote_urls.len(), 10);
}

#[test]
fn disable_embedded_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let disabled = transformer.disable_content(false, true);
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(disabled.embedded_urls.len(), 3);
}

#[test]
fn disable_all_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let disabled = transformer.disable_content(true, true);
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(disabled.remote_urls.len(), 10);
    assert_eq!(disabled.embedded_urls.len(), 3);
}

#[test]
fn disable_all_elements_uri_test() {
    let html = include_str!("../../tests/htmls/strip_uri_elements.html");
    let mut transformer = Transformer::new(html);
    let _ = transformer.disable_content(true, true);
    insta::assert_snapshot!(transformer.to_string());
}
