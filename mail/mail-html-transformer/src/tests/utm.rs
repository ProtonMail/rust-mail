#![allow(non_snake_case)]
use super::*;

#[test]
fn remove_from_url() {
    let url = "https://example.com/?UTM_SOURCE=example&utm_medium=example&utm_campaign=example&UserID=123";
    let new_url = strip_from_string(url).unwrap();
    assert_eq!(new_url.0.as_str(), "https://example.com/?UserID=123");

    let url = "panda"; // Invalid URL
    let new_url = strip_from_string(url);
    assert!(new_url.is_err());
}

#[test]
fn remove_with_transformer() {
    use crate::Transformer;
    use kuchikiki::traits::*;
    let input = r#"
<html>
    <body>
        <a href="https://ads.com?utm_source=tracker">bar</a>
    </body>
</html>
"#;

    let expected = r#"
<html>
    <body>
        <a href="https://ads.com/">bar</a>
    </body>
</html>
"#;

    // Parse and print so the results have the same formatting.
    let expected = kuchikiki::parse_html().one(expected).to_string();

    let mut transformer = Transformer::new(input);
    transformer.strip_utm();
    let output = transformer.to_string();
    assert_eq!(expected, output);
}
