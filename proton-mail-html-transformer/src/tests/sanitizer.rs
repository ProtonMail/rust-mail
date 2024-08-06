#![allow(non_snake_case)]

use crate::Transformer;

#[test]
fn acceptable_html() {
    let html = include_str!("../../tests/htmls/acceptable.html");

    let unsanitized_html = Transformer::new(html).strip_whitelist().to_string();
    let html = Transformer::new(html).strip_whitelist().to_string();
    assert_eq!(unsanitized_html, html);
}

#[test]
fn strip_bad_html() {
    let html = include_str!("../../tests/htmls/strip_bad.html");

    let html = Transformer::new(html).strip_whitelist().to_string();
    insta::assert_snapshot!(html);
}

#[test]
fn email_privacy_tester() {
    let html = include_str!("../../tests/htmls/email_privacy_tester.html");

    let html = Transformer::new(html).strip_whitelist().to_string();
    insta::assert_snapshot!(html);
}
