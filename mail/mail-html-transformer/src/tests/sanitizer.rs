#![allow(non_snake_case)]

use crate::Transformer;

#[test]
fn acceptable_html() {
    let html = include_str!("../../tests/htmls/acceptable.html");

    let unsanitized_html = Transformer::new(html).to_string();

    let mut t = Transformer::new(html);
    let _count = t.strip_whitelist();
    let html = t.to_string();
    assert_eq!(unsanitized_html, html);
}

#[test]
fn strip_bad_html() {
    let html = include_str!("../../tests/htmls/strip_bad.html");

    let mut t = Transformer::new(html);
    let _count = t.strip_whitelist();
    let html = t.to_string();
    insta::assert_snapshot!(html);
}

#[test]
fn email_privacy_tester() {
    let html = include_str!("../../tests/htmls/email_privacy_tester.html");

    let mut t = Transformer::new(html);
    let _count = t.strip_whitelist();
    let html = t.to_string();
    insta::assert_snapshot!(html);
}
#[test]
fn style_elements_are_stripped_away() {
    let html = include_str!("../../tests/htmls/styled.html");

    let mut t = Transformer::new(html);
    t.strip_whitelist();

    let html = t.to_string();
    insta::assert_snapshot!(html);
}
