#![allow(clippy::needless_raw_string_hashes)]

use crate::{
    InsertLinkToken, Transformer,
    transforms::{
        ColorMode,
        styles::{BrowserCapabilities, IncludeFullStaticCss},
    },
};
#[test]
fn inject_style() {
    let html = include_str!("../../tests/htmls/empty.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        "",
        ColorMode::LightMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: false,
        },
        IncludeFullStaticCss::No,
        &[],
    );
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
    html.inject_dark_mode(
        "",
        ColorMode::LightMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: false,
        },
        IncludeFullStaticCss::No,
        &[],
    );
    insta::assert_snapshot!(html.to_string());
}

// For more tests regarding dark mode look into the module:
mod dark_mode;

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
            * httpssp://ads.com?utm_source=tracker
            * httpsp://ads.com?utm_source=tracker
            * mailto:foo@bar

            Outro

        "#;

    let mut html = Transformer::new_text_plain(html);
    html.insert_links(InsertLinkToken(()));
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn plain_text_forwarding_is_preserved() {
    let input = r#"
TEST

Signature

------- Forwarded Message -------
From: Foo@bar.com
Date: On Wednesday, April 2nd, 2025 at 18:27
Subject: Lorem ipsum dolor sit amet
To: lorem@ipsum.ch <lorem@ipsum.ch>


> Lorem ipsum dolor sit amet, consectetur adipiscing elit. Etiam nec convallis lorem. Fusce
> scelerisque turpis eu tincidunt luctus. Sed sem tellus, cursus non arcu id, vestibulum
> tempus arcu. Vivamus non odio a diam sollicitudin vestibulum id vitae mauris. Sed consequat
> felis est, sed auctor eros consectetur eget. Sed augue urna, faucibus euismod orci eget,
> tincidunt porttitor nibh. Nulla ultricies hendrerit accumsan. Nulla suscipit vel dui eu iaculis.
> Nam convallis, urna nec scelerisque venenatis, justo sem faucibus metus, et mattis nisl purus ac
> nulla.
>
> ====================================================
>
> X >Y && Y < X
>
> ----------
> <pre> <>& & <> </pre>
"#;
    let html = Transformer::new_text_plain(input);
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn move_styles_to_body() {
    let input = r#"
       <html>
       <head>
       <title>Title</title>
       <style> body { color: red; }  </style>
       <style> .a { color: black; } </style>
       </head>
       <body>
       <span>Text in body</span>
       </body>
       </html>
    "#;

    let mut html = Transformer::new(input);
    html.move_styles_to_body();
    insta::assert_snapshot!(html.to_string());
}
