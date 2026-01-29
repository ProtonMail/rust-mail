use crate::Transformer;
use test_case::test_case;

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

    assert_eq!(disabled.remote_urls.len(), 10);
    assert_eq!(disabled.embedded_urls.len(), 3);

    let html = transformer.to_string();
    assert!(html.contains("https://foo.bar.com/img.png"));
    assert!(html.contains("cid:1234"));
}

#[test]
fn count_remote_urls_without_mutation_hide_embedded_only() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    let disabled = transformer.disable_content(false, true);

    assert_eq!(disabled.remote_urls.len(), 10);
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

#[test_case(false, false)]
#[test_case(false, true)]
#[test_case(true, false)]
#[test_case(true, true)]
fn do_not_count_links_bases_areas_as_remote_content(no_remote: bool, no_embedded: bool) {
    let input = r#"
        <a href="https://example.com" target="_blank">Link</a>
        <base href="https://example.com">
        <area shape="rect" coords="0,0,100,100" href="https://example.com">
    "#;
    let mut transformer = Transformer::new(input);
    let disabled = transformer.disable_content(no_remote, no_embedded);
    assert_eq!(disabled.remote_urls.len(), 0);
    assert_eq!(disabled.embedded_urls.len(), 0);
}

#[test_case(false, false)]
#[test_case(false, true)]
#[test_case(true, false)]
#[test_case(true, true)]
fn treat_links_bases_areas_as_remote_content_if_it_has_background_image(
    no_remote: bool,
    no_embedded: bool,
) {
    let input = r#"
        <a href="https://example.com" target="_blank" style="background: url('https://tracking.com/image.png');">Link</a>
        <base href="https://example.com" style="background: url('https://tracking.com/image2.png');" >
        <area shape="rect" coords="0,0,100,100" href="https://example.com" style="background: url('https://tracking.com/image3.png');">
    "#;
    let mut transformer = Transformer::new(input);
    let disabled = transformer.disable_content(no_remote, no_embedded);
    assert_eq!(disabled.remote_urls.len(), 3);
    assert_eq!(disabled.embedded_urls.len(), 0);
}

#[test_case(false, false)]
#[test_case(false, true)]
#[test_case(true, false)]
#[test_case(true, true)]
fn treat_links_bases_areas_as_embedded_content_if_it_has_background_image(
    no_remote: bool,
    no_embedded: bool,
) {
    let input = r#"
        <a href="https://example.com" target="_blank" style="background: url(cid:1234);">Link</a>
        <base href="https://example.com" style="background: url(cid:5678);" >
        <area shape="rect" coords="0,0,100,100" href="https://example.com" style="background: url(cid:91011);">
    "#;
    let mut transformer = Transformer::new(input);
    let disabled = transformer.disable_content(no_remote, no_embedded);
    assert_eq!(disabled.remote_urls.len(), 0);
    assert_eq!(disabled.embedded_urls.len(), 3);
}
