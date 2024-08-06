#![allow(non_snake_case)]

use crate::Transformer;

#[test]
fn inject_style() {
    let html = include_str!("../../tests/htmls/empty.html");
    let html = Transformer::new(html).inject_style().to_string();
    insta::assert_snapshot!(html);
}

#[test]
fn inject_style_fail() {
    let html = r"
        <div>
          ain't no `head` here boss
        </div>
        ";
    let html = Transformer::new(html).inject_style().to_string();
    insta::assert_snapshot!(html);
}

#[test]
fn add_noreferrer() {
    let html = r#"
        <div>
          <a href="proton.me"/>
          <a href="proton.me" rel="foobar"/>
        </div>
        "#;
    let html = Transformer::new(html).add_noreferrer().to_string();
    insta::assert_snapshot!(html);
}
