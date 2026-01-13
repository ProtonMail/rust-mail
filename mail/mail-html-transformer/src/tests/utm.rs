use super::*;

#[test]
fn remove_from_url() {
    let url = "https://example.com/?UTM_SOURCE=example&utm_medium=example&utm_campaign=example&UserID=123";
    let new_url = strip_from_string(url).unwrap();
    assert_eq!(new_url.as_str(), "https://example.com/?UserID=123");

    let url = "panda"; // Invalid URL
    let new_url = strip_from_string(url);
    assert!(new_url.is_err());
}

#[test]
fn preserve_params_without_values() {
    let url = "https://example.com?foo&bar=1";
    let new_url = strip_from_string(url).unwrap();
    assert_eq!(new_url.as_str(), "https://example.com/?foo&bar=1");
}

#[test]
fn test_transformer_utm() {
    let body = r#"
        <a href="https://example.com?foo=1">Example</a>
        <a href="https://example.com/?utm_source=example&utm_medium=example&utm_campaign=example">Tracker Example</a>
    "#;

    let mut transformer = crate::Transformer::new(body);
    let results = transformer.strip_utm();
    let body = transformer.extract_body();

    insta::assert_snapshot!(body);
    insta::assert_debug_snapshot!(results);
}
