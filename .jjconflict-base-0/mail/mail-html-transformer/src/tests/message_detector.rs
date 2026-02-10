use super::*;

mod message_detector_test_messages;

use html5ever::tendril::TendrilSink;
use insta::assert_snapshot;

fn strip_blockquote_strings(input: &str) -> (String, String) {
    let document = kuchikiki::parse_html().one(input);
    let SplitDoc {
        message,
        blockquote,
    } = strip_blockquote(document);

    let blockquote = blockquote.map_or_else(String::new, |e| e.to_string());

    (message.to_string(), blockquote)
}

#[test]
fn detect_blockquote_or_signature() {
    let input = include_str!("./html/blockquote_or_signature.html");
    let (before, after) = strip_blockquote_strings(input);

    assert!(!before.contains("On Tuesday"));
    assert!(after.contains("On Tuesday"));
}

#[test]
fn should_take_the_last_element_containing_text_in_case_of_sibling_blockquotes() {
    let input = r#"Email content
<div class="protonmail_quote">
    blockquote1
</div>
<div class="protonmail_quote">
    blockquote2
</div>"#;

    let (before, after) = strip_blockquote_strings(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(!before.contains("blockquote2"));
    assert!(after.contains("blockquote2"));
    assert!(!after.contains("blockquote1"));
}

#[test]
fn should_take_the_last_element_containing_an_image_in_cas_of_sibling_blockquotes() {
    let input = r#"Email content
<div class="protonmail_quote">
    blockquote1
</div>
<div class="protonmail_quote">
    <span class="proton-image-anchor" />
</div>"#;

    let (before, after) = strip_blockquote_strings(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(!before.contains("proton-image-anchor"));
    assert!(after.contains("proton-image-anchor"));
    assert!(!after.contains("blockquote1"));
}

#[test]
fn should_display_nothing_in_blockquote_when_there_is_text_after_blockquotes() {
    let input = r#"Email content
<div class="protonmail_quote">
    blockquote1
</div>
text after blockquote"#;

    let (before, after) = strip_blockquote_strings(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(before.contains("text after blockquote"));
    assert!(after.is_empty());
}

#[test]
fn should_display_nothing_in_blockquote_when_there_is_an_image_after_blockquotes() {
    let input = r#"Email content
<div class="protonmail_quote">
    blockquote1
</div>
<span class="proton-image-anchor" />"#;

    let (before, after) = strip_blockquote_strings(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(before.contains("proton-image-anchor"));
    assert!(after.is_empty());
}

#[test]
fn should_find_blockquote_in_mail() {
    let mut failed = vec![];
    for (name, mail) in message_detector_test_messages::DEFAULT {
        let (_, after) = strip_blockquote_strings(mail);
        if after.is_empty() {
            failed.push(name);
        }
    }

    assert!(
        failed.is_empty(),
        "finding blockquote failed for messages {failed:#?}"
    );
}

#[test]
fn should_display_nothing_in_blockquote_when_it_is_not_last_important_element() {
    let original = include_str!("./html/interleaved.html");
    let (sanitized, blockquote) = strip_blockquote_strings(original);

    assert_snapshot!(sanitized);
    assert_snapshot!(blockquote);
}
