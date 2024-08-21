use super::*;

#[test]
fn pathologic_nested() {
    // This test includes a very deeply nested html that we can use for stack overflow
    // detection
    let doc = include_str!("../../tests/htmls/nested.html");
    let mut t = Transformer::new(doc);
    _ = t.strip_utm();
    _ = t.enable_remote_content();
    _ = t.disable_remote_content();
    _ = t.inject_ios_content_size();
    _ = t.strip_whitelist();
    _ = t.inject_style();
    _ = t.add_noreferrer();
    _ = t.proxy_images("THISISATOKEN");
    _ = t.strip_blockquote();
    _ = t.insert_links();
    // .to_string(); // https://github.com/servo/html5ever/issues/290
}
