#![allow(non_snake_case)]
#![allow(clippy::needless_raw_string_hashes)]

use crate::{InsertLinkToken, Transformer};
#[test]
fn inject_style() {
    let html = include_str!("../../tests/htmls/empty.html");
    let mut html = Transformer::new(html);
    html.inject_style();
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_no_head() {
    let html = r"
        <div>
          ain't no `head` here boss
        </div>
        ";

    let mut html = Transformer::new(html);
    html.inject_style();
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn add_noreferrer() {
    let html = r#"
        <div>
          <a href="proton.me"/>
          <a href="proton.me" rel="foobar"/>
        </div>
        "#;
    let mut html = Transformer::new(html);
    html.add_noreferrer();
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn insert_links() {
    let html = r#"
        <div id="1"> this is some content without a link </div>
        <div id="2">https://proton.me</div>
        <div id="3"> this is some content with a link to https://proton.me :) </div>
        <div id="4"> strippin' balls https://ads.com?utm_source=tracker </div>
        <div id="5"> incompete url not handled: proton.me </div>
        <div id="6"> empty url not matched: https: </div>
        <div id="7"> empty url not matched: mailto: </div>
        <div id="8"> localhost http://localhost </div>
        <div id="9"> ip http://127.0.0.1 </div>
        <div id="10"> mailto:foo@bar </div>
        "#;
    let mut html = Transformer::new(html);
    html.insert_links(InsertLinkToken(()));
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn insert_links_text() {
    let html = r#"
            Intro,

            Blah blabh blah, find reports at https://proton.me etc..

            See also:
            * https://127.0.0.1
            * https://ads.com?utm_source=tracker
            * mailto:foo@bar

            Outro

        "#;

    let mut html = Transformer::new(html);
    html.insert_links(InsertLinkToken(()));
    insta::assert_snapshot!(html.to_string());
}
#[test]
fn proxy_images() {
    let html = r#"
        <body>
        <img id="1" src="bad url">
        <img id="2" src="https://ads.com">
        <img id="2" src="https://ads.com?utm_source=tracker">
        </body>
        "#;
    let mut html = Transformer::new(html);
    html.proxy_images("MYTOKEN123");
    insta::assert_snapshot!(html.to_string());
}
