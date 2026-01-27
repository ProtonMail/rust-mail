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

#[test]
fn count_remote_urls_without_mutation() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let disabled = transformer.disable_content(false, false);

    assert_eq!(disabled.remote_urls.len(), 12);
    assert_eq!(disabled.embedded_urls.len(), 3);

    let html = transformer.to_string();
    assert!(html.contains("https://foo.bar.com/img.png"));
    assert!(html.contains("cid:1234"));
}

#[test]
fn count_remote_urls_without_mutation_hide_embedded_only() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let disabled = transformer.disable_content(false, true);

    assert_eq!(disabled.remote_urls.len(), 12);
    assert_eq!(disabled.embedded_urls.len(), 3);

    let html = transformer.to_string();
    assert!(html.contains("https://foo.bar.com/img.png"));
    assert!(!html.contains("cid:1234"));
}

#[test]
fn block_remote_url_in_css_image_set() {
    let input = r#"
        <div style="background: image-set('https://tracking.com/image.png');">Hello proton user!</div>
    "#;
    let mut transformer = Transformer::new(input);
    let disabled = transformer.disable_content(true, true);
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(disabled.remote_urls.len(), 1);
    assert_eq!(disabled.embedded_urls.len(), 0);
}
