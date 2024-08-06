use super::*;

#[test]
fn pathologic_nested() {
    // This test includes a very deeply nested html that we can use for stack overflow
    // detection
    let doc = include_str!("../../tests/htmls/nested.html");
    Transformer::new(doc)
        .strip_utm()
        .enable_remote_content()
        .disable_remote_content()
        .inject_ios_content_size()
        .strip_whitelist()
        .inject_style()
        .add_noreferrer()
        .proxy_images("THISISATOKEN")
        .insert_links();
    // .to_string(); // https://github.com/servo/html5ever/issues/290
}
