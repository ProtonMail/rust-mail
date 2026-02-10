use core::str;

use mail_parser::decoders::base64::base64_decode;
use proton_crypto_inbox_mime::write::InboxMimeBuilder;
use proton_crypto_inbox_mime::{MimeProcessor, ProcessMime, ProcessedBodyType};

pub const WEB_EXAMPLE: &str = r#"Content-Type: multipart/mixed;boundary=---------------------d600e8c7e563966198cbd667aac541cd

-----------------------d600e8c7e563966198cbd667aac541cd
Content-Type: multipart/alternative;boundary=---------------------1eee48e732630bbd2951a9c467f1f4e8

-----------------------1eee48e732630bbd2951a9c467f1f4e8
Content-Transfer-Encoding: quoted-printable
Content-Type: text/plain;charset=utf-8


Hello


Sent with Proton Mail secure email.
-----------------------1eee48e732630bbd2951a9c467f1f4e8
Content-Type: multipart/related;boundary=---------------------ec904825800e4ca3c42b0217f886048f

-----------------------ec904825800e4ca3c42b0217f886048f
Content-Type: text/html;charset=utf-8
Content-Transfer-Encoding: base64

PGRpdiBzdHlsZT0iZm9udC1mYW1pbHk6IEFyaWFsLCBzYW5zLXNlcmlmOyBmb250LXNpemU6IDE0
cHg7Ij48aW1nIGFsdD0icHJvdG9uX2xvZ28ucG5nIiBjbGFzcz0icHJvdG9uLWVtYmVkZGVkIiBz
cmM9ImNpZDozNmZmOWNkOUBwcm90b24ubWUiPjxicj48L2Rpdj48ZGl2IHN0eWxlPSJmb250LWZh
bWlseTogQXJpYWwsIHNhbnMtc2VyaWY7IGZvbnQtc2l6ZTogMTRweDsiPkhlbGxvPC9kaXY+PGRp
diBzdHlsZT0iZm9udC1mYW1pbHk6IEFyaWFsLCBzYW5zLXNlcmlmOyBmb250LXNpemU6IDE0cHg7
Ij48YnI+PC9kaXY+CjxkaXYgY2xhc3M9InByb3Rvbm1haWxfc2lnbmF0dXJlX2Jsb2NrIiBzdHls
ZT0iZm9udC1mYW1pbHk6IEFyaWFsLCBzYW5zLXNlcmlmOyBmb250LXNpemU6IDE0cHg7Ij4KICAg
IDxkaXYgY2xhc3M9InByb3Rvbm1haWxfc2lnbmF0dXJlX2Jsb2NrLXVzZXIgcHJvdG9ubWFpbF9z
aWduYXR1cmVfYmxvY2stZW1wdHkiPgogICAgICAgIAogICAgICAgICAgICA8L2Rpdj4KICAgIAog
ICAgICAgICAgICA8ZGl2IGNsYXNzPSJwcm90b25tYWlsX3NpZ25hdHVyZV9ibG9jay1wcm90b24i
PgogICAgICAgIFNlbnQgd2l0aCA8YSB0YXJnZXQ9Il9ibGFuayIgaHJlZj0iaHR0cHM6Ly9wcm90
b24ubWUvIj5Qcm90b24gTWFpbDwvYT4gc2VjdXJlIGVtYWlsLgogICAgPC9kaXY+CjwvZGl2Pgo=
-----------------------ec904825800e4ca3c42b0217f886048f--
-----------------------1eee48e732630bbd2951a9c467f1f4e8--
-----------------------d600e8c7e563966198cbd667aac541cd
Content-Type: image/png; filename="proton_logo.png"; name="proton_logo.png"
Content-Transfer-Encoding: base64
Content-Disposition: inline; filename="proton_logo.png"; name="proton_logo.png"
Content-ID: <36ff9cd9@proton.me>

iVBORw0KGgoAAAANSUhEUgAAAF4AAABeCAIAAAAlsDQ5AAAAAXNSR0IArs4c6QAAAFBlWElmTU0A
KgAAAAgAAgESAAMAAAABAAEAAIdpAAQAAAABAAAAJgAAAAAAA6ABAAMAAAABAAEAAKACAAQAAAAB
AAAAXqADAAQAAAABAAAAXgAAAADdKSSmAAABWWlUWHRYTUw6Y29tLmFkb2JlLnhtcAAAAAAAPHg6
eG1wbWV0YSB4bWxuczp4PSJhZG9iZTpuczptZXRhLyIgeDp4bXB0az0iWE1QIENvcmUgNi4wLjAi
PgogICA8cmRmOlJERiB4bWxuczpyZGY9Imh0dHA6Ly93d3cudzMub3JnLzE5OTkvMDIvMjItcmRm
LXN5bnRheC1ucyMiPgogICAgICA8cmRmOkRlc2NyaXB0aW9uIHJkZjphYm91dD0iIgogICAgICAg
ICAgICB4bWxuczp0aWZmPSJodHRwOi8vbnMuYWRvYmUuY29tL3RpZmYvMS4wLyI+CiAgICAgICAg
IDx0aWZmOk9yaWVudGF0aW9uPjE8L3RpZmY6T3JpZW50YXRpb24+CiAgICAgIDwvcmRmOkRlc2Ny
aXB0aW9uPgogICA8L3JkZjpSREY+CjwveDp4bXBtZXRhPgoZXuEHAAAHYElEQVR4Ae2ca2wUVRTH
//PY7T7aslDa0vJsRB5CgQiIQdGQiCY1ghgfRA1KFIkkkmCMBqkhkqDRYEjE+OCDEvgCfiACJloj
goiCAZEipRZKhT6AtunDdrf73vFOC9sF9s6dZe6sLDv30/Sec8+99zdnzn1uhdcqFFgpGQExWaaV
pxKw0FD9wEJjoaESoAosr7HQUAlQBZbXWGioBKgCy2ssNFQCVIHlNRYaKgGqwPIaCw2VAFVgeQ0V
jUyVpCgIeuFrSbEMS10QINog5kB2wuaEnAMIrDL85HzQEC4LlmH+o4hG+DUNUBT4fejtxeVG1B7H
4U9hL4FrOGwunrXQbPFBEwsjfygKCmm1GM0vvxsLHseq9ThTgwPf4qcP4JkGu9uoWe3y3GKNEtOu
iI90whS88hY+P4s5i9B6EDGuTnpdE7mhuc6uqX96CrBsNT46CdmOQI9ZVWUkmgEYo8qwaQ9mVqCn
yRQ6GYyG8CBD2Io1WPQGus7xp8MnDPNvVyoWFy9Vx7I9m5A/MpViLN3M9pp47554AbMWI9gbz+Dw
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
LAAAAABJRU5ErkJggg==
-----------------------d600e8c7e563966198cbd667aac541cd
Content-Type: application/rls-services+xml; filename="attachment.rs"; name="attachment.rs"
Content-Transfer-Encoding: base64
Content-Disposition: attachment; filename="attachment.rs"; name="attachment.rs"

aGVsbG8=
-----------------------d600e8c7e563966198cbd667aac541cd--
"#;

const EXAMPLE_IMAGE: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAF4AAABeCAIAAAAlsDQ5AAAAAXNSR0IArs4c6QAAAFBlWElmTU0A
KgAAAAgAAgESAAMAAAABAAEAAIdpAAQAAAABAAAAJgAAAAAAA6ABAAMAAAABAAEAAKACAAQAAAAB
AAAAXqADAAQAAAABAAAAXgAAAADdKSSmAAABWWlUWHRYTUw6Y29tLmFkb2JlLnhtcAAAAAAAPHg6
eG1wbWV0YSB4bWxuczp4PSJhZG9iZTpuczptZXRhLyIgeDp4bXB0az0iWE1QIENvcmUgNi4wLjAi
PgogICA8cmRmOlJERiB4bWxuczpyZGY9Imh0dHA6Ly93d3cudzMub3JnLzE5OTkvMDIvMjItcmRm
LXN5bnRheC1ucyMiPgogICAgICA8cmRmOkRlc2NyaXB0aW9uIHJkZjphYm91dD0iIgogICAgICAg
ICAgICB4bWxuczp0aWZmPSJodHRwOi8vbnMuYWRvYmUuY29tL3RpZmYvMS4wLyI+CiAgICAgICAg
IDx0aWZmOk9yaWVudGF0aW9uPjE8L3RpZmY6T3JpZW50YXRpb24+CiAgICAgIDwvcmRmOkRlc2Ny
aXB0aW9uPgogICA8L3JkZjpSREY+CjwveDp4bXBtZXRhPgoZXuEHAAAHYElEQVR4Ae2ca2wUVRTH
//PY7T7aslDa0vJsRB5CgQiIQdGQiCY1ghgfRA1KFIkkkmCMBqkhkqDRYEjE+OCDEvgCfiACJloj
goiCAZEipRZKhT6AtunDdrf73vFOC9sF9s6dZe6sLDv30/Sec8+99zdnzn1uhdcqFFgpGQExWaaV
pxKw0FD9wEJjoaESoAosr7HQUAlQBZbXWGioBKgCy2ssNFQCVIHlNRYaKgGqwPIaCw2VAFVgeQ0V
jUyVpCgIeuFrSbEMS10QINog5kB2wuaEnAMIrDL85HzQEC4LlmH+o4hG+DUNUBT4fejtxeVG1B7H
4U9hL4FrOGwunrXQbPFBEwsjfygKCmm1GM0vvxsLHseq9ThTgwPf4qcP4JkGu9uoWe3y3GKNEtOu
iI90whS88hY+P4s5i9B6EDGuTnpdE7mhuc6uqX96CrBsNT46CdmOQI9ZVWUkmgEYo8qwaQ9mVqCn
yRQ6GYyG8CBD2Io1WPQGus7xp8MnDPNvVyoWFy9Vx7I9m5A/MpViLN3M9pp47554AbMWI9gbz+Dw
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
LAAAAABJRU5ErkJggg==";

#[test]
fn test_write_and_compare_to_web() {
    let text = "Hello


Sent with Proton Mail secure email.";
    let html = r#"<div style="font-family: Arial, sans-serif; font-size: 14px;"><img alt="proton_logo.png" class="proton-embedded" src="cid:36ff9cd9@proton.me"><br></div><div style="font-family: Arial, sans-serif; font-size: 14px;">Hello</div><div style="font-family: Arial, sans-serif; font-size: 14px;"><br></div>
<div class="protonmail_signature_block" style="font-family: Arial, sans-serif; font-size: 14px;">
    <div class="protonmail_signature_block-user protonmail_signature_block-empty">
        
            </div>
    
            <div class="protonmail_signature_block-proton">
        Sent with <a target="_blank" href="https://proton.me/">Proton Mail</a> secure email.
    </div>
</div>
"#;

    let inline_attachment =
        base64_decode(EXAMPLE_IMAGE.as_bytes()).expect("Unable to read file as bytes");
    let attachment = b"hello".to_vec();
    let mut data = Vec::new();
    InboxMimeBuilder::new()
        .text_body(text)
        .begin_html_body(html)
        .end_html_body()
        .inline_attachment(
            "36ff9cd9@proton.me",
            "proton_logo.png",
            Some("image/png"),
            inline_attachment,
        )
        .attachment(
            "attachment.rs",
            Some("application/rls-services+xml"),
            attachment,
        )
        .write_to(&mut data)
        .unwrap();

    let processed = MimeProcessor::process_mime("message_id", &data).unwrap();
    let processed_expected =
        MimeProcessor::process_mime("message_id", WEB_EXAMPLE.as_bytes()).unwrap();
    assert_eq!(processed.body, processed_expected.body);
    assert_eq!(
        processed.encrypted_subject,
        processed_expected.encrypted_subject
    );
    assert_eq!(processed.mime_body_type, processed_expected.mime_body_type);

    // First attachment should be equal.
    assert_eq!(
        processed.attachments.first().unwrap(),
        processed_expected.attachments.first().unwrap()
    );

    // Second has random content ids.
    let current = processed.attachments.get(1).unwrap();
    let expected = processed_expected.attachments.get(1).unwrap();
    assert_eq!(current.data, expected.data);
    assert_eq!(current.mime_type, expected.mime_type);
    assert_eq!(current.size, expected.size);
    assert_eq!(current.name, expected.name);
}

#[test]
fn test_write_html() {
    let text = "Hello";
    let html = r#"<html><body><h1>Hello</h1><img src="cid:image1"></body></html>"#;
    let attachment_data = b"hello";

    let mut data = Vec::new();
    InboxMimeBuilder::new()
        .text_body(text)
        .begin_html_body(html)
        .inline_attachment(
            "inline_html",
            "inline_html_logo.png",
            Some("image/png"),
            b"inline_html".to_vec(),
        )
        .end_html_body()
        .inline_attachment(
            "inline_attachment",
            "inline_attachment_logo.png",
            Some("image/png"),
            b"inline_attachment".to_vec(),
        )
        .attachment(
            "attachment",
            Some("application/test"),
            attachment_data.to_vec(),
        )
        .write_to(&mut data)
        .unwrap();

    let processed = MimeProcessor::process_mime("message_id", &data).unwrap();
    assert_eq!(processed.body, html);
    assert_eq!(processed.mime_body_type, ProcessedBodyType::Html);

    // inline html
    let attachment = processed.attachments.first().unwrap();
    assert_eq!(attachment.data, b"inline_html");
    assert_eq!(&attachment.mime_type, "image/png");
    assert_eq!(attachment.name, "inline_html_logo.png");
    assert_eq!(attachment.content_id, "inline_html");

    // inline all
    let attachment = processed.attachments.get(1).unwrap();
    assert_eq!(attachment.data, b"inline_attachment");
    assert_eq!(&attachment.mime_type, "image/png");
    assert_eq!(attachment.name, "inline_attachment_logo.png");
    assert_eq!(attachment.content_id, "inline_attachment");

    // attachment
    let attachment = processed.attachments.get(2).unwrap();
    assert_eq!(attachment.data, attachment_data);
    assert_eq!(&attachment.mime_type, "application/test");
    assert_eq!(attachment.name, "attachment");
}

#[test]
fn test_write_no_html() {
    let text = "Hello";
    let attachment_data = b"hello";

    let mut data = Vec::new();
    InboxMimeBuilder::new()
        .text_body(text)
        .attachment(
            "attachment",
            Some("application/test"),
            attachment_data.to_vec(),
        )
        .write_to(&mut data)
        .unwrap();

    let processed = MimeProcessor::process_mime("message_id", &data).unwrap();
    assert_eq!(processed.body, text);
    assert_eq!(processed.mime_body_type, ProcessedBodyType::Text);

    // attachment
    let attachment = processed.attachments.first().unwrap();
    assert_eq!(attachment.data, attachment_data);
    assert_eq!(&attachment.mime_type, "application/test");
    assert_eq!(attachment.name, "attachment");
}

#[test]
fn test_write_emojis() {
    let text = "🤩✨☄️🥲



Sent with [Proton Mail][1] secure email.

[1]: https://proton.me/mail/home";
    let expected_emojis = "=F0=9F=A4=A9=E2=9C=A8=E2=98=84=EF=B8=8F=F0=9F=A5=B2";

    let mut data = Vec::new();
    InboxMimeBuilder::new()
        .text_body(text)
        .write_to(&mut data)
        .unwrap();
    let mime_content = String::from_utf8(data.clone()).unwrap();
    assert!(mime_content.contains(expected_emojis));
}
