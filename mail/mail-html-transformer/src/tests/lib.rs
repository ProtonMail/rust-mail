use super::*;

#[test]
fn pathologic_nested() {
    // This test includes a very deeply nested html that we can use for stack overflow
    // detection
    let doc = include_str!("../../tests/htmls/nested.html");
    let mut t = Transformer::new(doc);
    t.strip_utm();
    t.disable_content(true, true);
    t.inject_ios_content_size();
    _ = t.strip_whitelist();
    t.inject_style(
        ColorMode::LightMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    _ = t.strip_blockquote();
    let tok = t.add_noreferrer();
    t.insert_links(tok);
    // .to_string(); // https://github.com/servo/html5ever/issues/290
}
