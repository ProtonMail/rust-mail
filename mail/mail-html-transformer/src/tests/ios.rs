#[test]
fn inject_with_existing_head_element() {
    let input = r"<html><head></head><body></body></html>";

    let expected = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=1.0"></head><body></body></html>"#;

    let mut transformer = crate::Transformer::new(input);
    transformer.inject_ios_content_size();
    let output = transformer.to_string();
    assert_eq!(expected, output);
}
#[test]
fn inject_without_existing_head_element() {
    let input = r"<html><body></body></html>";

    let expected = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=1.0"></head><body></body></html>"#;

    let mut transformer = crate::Transformer::new(input);
    transformer.inject_ios_content_size();
    let output = transformer.to_string();
    assert_eq!(expected, output);
}

#[test]
fn inject_without_existing_viewport_entry() {
    // Make sure it appears as the last entry if an existing meta item already exist.
    let input = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=0.0"></head><body></body></html>"#;

    // The parser outputs a closing meta tag only for the newly added element. Existing meta
    // elements do not have this issue.
    let expected = r#"<html><head><meta name="viewport" content="width=device-width, initial-scale=0.0"><meta name="viewport" content="width=device-width, initial-scale=1.0"></head><body></body></html>"#;

    let mut transformer = crate::Transformer::new(input);
    transformer.inject_ios_content_size();
    let output = transformer.to_string();
    assert_eq!(expected, output);
}
