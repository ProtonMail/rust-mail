use super::*;

mod message_detector_test_messages;

use html5ever::tendril::TendrilSink;

fn locate_blockquote_strings(input: &str) -> (String, String) {
    let document = kuchikiki::parse_html().one(input);
    let SplitDoc {
        message,
        blockquote,
    } = locate_blockquote(document);

    let blockquote = match blockquote {
        Some(e) => e.to_string(),
        None => String::new(),
    };

    (message.to_string(), blockquote)
}

const INPUT_1: &str = r#"<div style="font-family: verdana; font-size: 20px;">
    <div style="font-family: verdana; font-size: 20px;"><br></div>
    <div class="protonmail_signature_block protonmail_signature_block-empty" style="font-family: verdana; font-size: 20px;">
        <div class="protonmail_signature_block-user protonmail_signature_block-empty"></div>
        <div class="protonmail_signature_block-proton protonmail_signature_block-empty"></div>
    </div>
    <div style="font-family: verdana; font-size: 20px;"><br></div>
    <div class="protonmail_quote">
        On Tuesday, January 4th, 2022 at 17:13, Swiip - Test account &lt;swiip.test@protonmail.com&gt; wrote:<br>
        <blockquote class="protonmail_quote" type="cite">
            <div style="font-family: verdana; font-size: 20px;">
                <div style="font-family: verdana; font-size: 20px;">test</div>
                <div class="protonmail_signature_block protonmail_signature_block-empty" style="font-family: verdana; font-size: 20px;">
                    <div class="protonmail_signature_block-user protonmail_signature_block-empty"></div>
                    <div class="protonmail_signature_block-proton protonmail_signature_block-empty"></div>
                </div>
            </div>
        </blockquote><br>
    </div>
</div>"#;

#[test]
fn detect_blockquote_or_signature() {
    let (before, after) = locate_blockquote_strings(INPUT_1);

    assert!(!before.contains("On Tuesday"));
    assert!(after.contains("On Tuesday"));
}

#[test]
fn strip_blockquote_returns_true() {
    let document = kuchikiki::parse_html().one(INPUT_1);
    assert!(strip_blockquote(document));
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

    let (before, after) = locate_blockquote_strings(input);

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

    let (before, after) = locate_blockquote_strings(input);

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

    let (before, after) = locate_blockquote_strings(input);

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

    let (before, after) = locate_blockquote_strings(input);

    assert!(before.contains("Email content"));
    assert!(before.contains("blockquote1"));
    assert!(before.contains("proton-image-anchor"));
    assert!(after.is_empty());
}

#[test]
fn should_find_blockquote_in_mail() {
    for (idx, &mail) in message_detector_test_messages::DEFAULT.iter().enumerate() {
        let (_, after) = locate_blockquote_strings(mail);
        assert!(
            !after.is_empty(),
            "blockquote failed for message {idx}\n{mail}"
        );
    }
}
