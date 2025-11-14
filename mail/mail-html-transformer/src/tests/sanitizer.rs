use crate::Transformer;
use crate::sanitizer::SanitizeStyles;

#[test]
fn acceptable_html() {
    let html = include_str!("../../tests/htmls/acceptable.html");

    let unsanitized_html = Transformer::new(html).to_string();

    let mut t = Transformer::new(html);
    let _count = t.strip_whitelist(SanitizeStyles::No);
    let html = t.to_string();
    assert_eq!(unsanitized_html, html);
}

#[test]
fn strip_bad_html() {
    let html = include_str!("../../tests/htmls/strip_bad.html");

    let mut t = Transformer::new(html);
    let _count = t.strip_whitelist(SanitizeStyles::No);
    let html = t.to_string();
    insta::assert_snapshot!(html);
}

#[test]
fn email_privacy_tester() {
    let html = include_str!("../../tests/htmls/email_privacy_tester.html");

    let mut t = Transformer::new(html);
    let _count = t.strip_whitelist(SanitizeStyles::No);
    let html = t.to_string();
    insta::assert_snapshot!(html);
}
#[test]
fn style_elements_are_kept() {
    let html = include_str!("../../tests/htmls/styled.html");

    let mut t = Transformer::new(html);
    t.strip_whitelist(SanitizeStyles::No);

    let html = t.to_string();
    insta::assert_snapshot!(html);
}

#[test]
fn strip_invalid_uris() {
    let html = include_str!("../../tests/htmls/strip_uri_elements.html");

    let mut t = Transformer::new(html);
    let _count = t.strip_whitelist(SanitizeStyles::No);
    let html = t.to_string();
    insta::assert_snapshot!(html);
}

#[test]
fn sanitize_styles_yes_removes_style_attributes() {
    let html = r#"<p style="color:red;" bgcolor="blue" align="center">Hello</p>"#;

    let mut t = Transformer::new(html);
    t.strip_whitelist(SanitizeStyles::Yes);
    let result = t.to_string();

    insta::assert_snapshot!(result);
}

#[test]
fn sanitize_styles_yes_removes_data_proton_original_style() {
    let html = r#"<div data-proton-original-style="color:blue;" class="content">Content</div>"#;

    let mut t = Transformer::new(html);
    t.strip_whitelist(SanitizeStyles::Yes);
    let result = t.to_string();

    insta::assert_snapshot!(result);
}

#[test]
fn sanitize_styles_yes_removes_style_elements() {
    let html =
        r"<html><head><style>.red {color: red;}</style></head><body><p>Hello</p></body></html>";

    let mut t = Transformer::new(html);
    t.strip_whitelist(SanitizeStyles::Yes);
    let result = t.to_string();

    insta::assert_snapshot!(result);
}

#[test]
fn sanitize_styles_no_preserves_style_attributes() {
    let html = r#"<p style="color:red;" bgcolor="blue">Hello</p>"#;

    let mut t = Transformer::new(html);
    t.strip_whitelist(SanitizeStyles::No);
    let result = t.to_string();

    insta::assert_snapshot!(result);
}

#[test]
fn sanitize_styles_no_preserves_style_elements() {
    let html =
        r"<html><head><style>.red {color: red;}</style></head><body><p>Hello</p></body></html>";

    let mut t = Transformer::new(html);
    t.strip_whitelist(SanitizeStyles::No);
    let result = t.to_string();

    insta::assert_snapshot!(result);
}

#[test]
fn sanitize_styles_yes_removes_srcset() {
    // Required by android
    let html =
        r#"<img src="image.jpg" srcset="image-320w.jpg 320w, image-480w.jpg 480w" alt="test">"#;

    let mut t = Transformer::new(html);
    t.strip_whitelist(SanitizeStyles::Yes);
    let result = t.to_string();

    insta::assert_snapshot!(result);
}

#[test]
fn sanitize_pasted_content_comprehensive() {
    let html = r##"<html>
            <head><style>.test {color: red;}</style></head>
            <body>
                <div style="margin:10px;" bgcolor="#fff" align="left" width="100%" height="50">
                    <p color="blue" data-proton-original-style="font-size:14px;" class="content">
                        Pasted content with styles
                    </p>
                    <table border="1" cellpadding="5" cellspacing="2">
                        <tr><td>Cell</td></tr>
                    </table>
                </div>
            </body>
        </html>"##;

    let mut t = Transformer::new(html);
    t.strip_whitelist(SanitizeStyles::Yes);
    let result = t.to_string();

    insta::assert_snapshot!(result);
}
