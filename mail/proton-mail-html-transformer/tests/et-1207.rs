use proton_mail_html_transformer::Transformer;

#[test]
fn et_1207() {
    let text = "
Test  test
test

Sent with Proton Mail secure email.

------- Original Message -------

> This is a quote
";

    let mut t = Transformer::new_text_plain(text);
    let processed = t.to_string();

    insta::assert_snapshot!(processed);

    let processed = t.strip_blockquote().to_string();
    insta::assert_snapshot!(processed);
}
