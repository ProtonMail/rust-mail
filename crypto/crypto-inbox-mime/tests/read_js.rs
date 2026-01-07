//! Tests from <https://github.com/ProtonMail/pmcrypto/blob/main/test/message/processMIME.spec.ts>
use proton_crypto::crypto::*;
use proton_crypto::new_pgp_provider;
use proton_crypto::utils::to_canonicalized_string;
use proton_crypto_inbox_mime::{MimeProcessor, MimeSignatureVerifier, ProcessMime};

pub const KEY: &str = r"-----BEGIN PGP PUBLIC KEY BLOCK-----
Version: OpenPGP.js v4.4.6
Comment: https://openpgpjs.org

xjMEXG6rNhYJKwYBBAHaRw8BAQdA63eiHJ6ylmHXwDzvNoBXDx3UkaF6rm3d
kToIFs8KYGnNG0pvbiBTbWl0aCA8am9uQGV4YW1wbGUuY29tPsJ3BBAWCgAf
BQJcbqs2BgsJBwgDAgQVCAoCAxYCAQIZAQIbAwIeAQAKCRACmBrNmWu7s6ig
AP4l4JUNFYP1lzje4+VB1oz3xgAJwDpIPnpvV4p6fVfCMQEAsfqvA6OdgLl+
MmVRBRXO1BUtkSxwS9zxzQfE/0NZ7QfOOARcbqs2EgorBgEEAZdVAQUBAQdA
4IcImEOmtilzNy6BvjyoHHtiukYZlb4/38iqQbzQxywDAQgHwmEEGBYIAAkF
AlxuqzYCGwwACgkQApgazZlru7OCeAD/Waa1g7t1DsrE8Di+ovD19Xs7js4R
82uvdzLBXafN8okBALL5uHCjG/gkJzHGun2Tj2MKO2ykR6gv6lVKo7jX75kD
=7vY3
-----END PGP PUBLIC KEY BLOCK-----";

pub const MULTIPART_SIGNED_MESSAGE: &str = r#"From: Jon Smith <jon@example.com>
To: Jon Smith <jon@example.com>
Mime-Version: 1.0
Content-Type: multipart/signed; boundary=bar; micalg=pgp-md5;
protocol="application/pgp-signature"

--bar
Content-Type: text/plain; charset=iso-8859-1
Content-Transfer-Encoding: quoted-printable

=A1Hola!

Did you know that talking to yourself is a sign of senility?

It's generally a good idea to encode lines that begin with
From=20because some mail transport agents will insert a greater-
than (>) sign, thus invalidating the signature.

Also, in some cases it might be desirable to encode any   =20
trailing whitespace that occurs on lines in order to ensure  =20
that the message signature is not invalidated when passing =20
a gateway that modifies such whitespace (like BITNET). =20

me   
--bar
Content-Type: application/pgp-signature

-----BEGIN PGP SIGNATURE-----
Version: OpenPGP.js v4.4.6
Comment: https://openpgpjs.org

wl4EARYKAAYFAlxurnwACgkQApgazZlru7OZ4gEA7gcIhNDZe9DurcA7I6Hb
J+mJL9vKtB5Ob4ponog5+ZYBAK6MCfmEImVCpdOlAIKmA9VRzQVLbW+Zm9cc
iwVC3WsC
=beyW
-----END PGP SIGNATURE-----

--bar--"#;

pub const INVALID_MULTIPART_SIGNED_MESSAGE: &str = r#"From: Jon Smith <jon@example.com>
To: Jon Smith <jon@example.com>
Mime-Version: 1.0
Content-Type: multipart/signed; boundary=bar; micalg=pgp-md5;
protocol="application/pgp-signature"

--bar
Content-Type: text/plain; charset=iso-8859-1
Content-Transfer-Encoding: quoted-printable

message with missing signature
--bar"#;

pub const MULTIPART_SIGNED_MESSAGE_BODY: &str = r"¡Hola!

Did you know that talking to yourself is a sign of senility?

It's generally a good idea to encode lines that begin with
From because some mail transport agents will insert a greater-
than (>) sign, thus invalidating the signature.

Also, in some cases it might be desirable to encode any    
trailing whitespace that occurs on lines in order to ensure   
that the message signature is not invalidated when passing  
a gateway that modifies such whitespace (like BITNET).  

me";

pub const MULTIPART_MESSAGE_WITH_ATTACHMENT: &str = r#"From: Some One <someone@example.com>
To: "Someone Else" <someone-else@example.com>
MIME-Version: 1.0
Content-Type: multipart/mixed; boundary="XXXXboundary text"

This is a multipart message in MIME format.

--XXXXboundary text
Content-Type: text/plain

this is the body text

--XXXXboundary text
Content-Type: text/plain;
Content-Disposition: attachment; filename="test.txt"

this is the attachment text

--XXXXboundary text--"#;

pub const EXTRA_MULTIPART_SIGNED_MESSAGE: &str = r#"From: Jon Smith <jon@example.com>
To: Jon Smith <jon@example.com>
Mime-Version: 1.0
Content-Type: multipart/signed; boundary=bar; micalg=pgp-md5;
protocol="application/pgp-signature"

--bar
Content-Type: text/plain; charset=iso-8859-1
Content-Transfer-Encoding: quoted-printable

hello
--bar
Content-Type: application/pgp-signature

-----BEGIN PGP SIGNATURE-----
Version: OpenPGP.js v4.4.6
Comment: https://openpgpjs.org

wl4EARYKAAYFAlxuurAACgkQApgazZlru7PubwEAkm2yNgMcCzv9YuW2zKEP
eo6TtHjWxF3GASwuZ/nMv/MBAJUDDC3PDfCIGyPKk2Pzf2t2co/+dEpW3vpx
euiL4uYD
=97+O
-----END PGP SIGNATURE-----

--bar
extra part
--bar--"#;

pub const MULTIPART_MESSAGE_WITH_ENCRYPTED_SUBJECT: &str = r#"From: "Some One" <someone@example.com>
To: "Someone Else" <someone-else@example.com>
Subject: ...
Mime-Version: 1.0
Content-Type: multipart/signed; micalg=pgp-sha256; protocol="application/pgp-signature"; boundary="------------w7atwMAiUaHQsKDKV5d0o0kr"

This is an OpenPGP/MIME signed message (RFC 4880 and 3156)
--------------w7atwMAiUaHQsKDKV5d0o0kr
Content-Type: multipart/mixed; boundary="------------nUB097wGzA443Ku03aYWQKqa"; protected-headers="v1"
Subject: Encrypted subject
From: "Some One" <someone@example.com>
To: "Someone Else" <someone-else@example.com>

--------------nUB097wGzA443Ku03aYWQKqa
Content-Type: text/plain; charset=iso-8859-1
Content-Transfer-Encoding: quoted-printable

hello
--------------nUB097wGzA443Ku03aYWQKqa--

--------------w7atwMAiUaHQsKDKV5d0o0kr
Content-Type: application/pgp-signature; name="OpenPGP_signature.asc"
Content-Description: OpenPGP digital signature
Content-Disposition: attachment; filename="OpenPGP_signature"

-----BEGIN PGP SIGNATURE-----

wnUEARYKAAYFAmIwlfMAIQkQdqGsuYvE1jgWIQRGvajOG9a8ZbdysiN2oay5
i8TWOBX5AP0V5H79/eiraXKKBCvpqwcEzrv1DHfhvrjTHk9L6PIadgD/fXdv
WTyjgksKkPV68HhW1CIKZ4JIMe726uldjP6tgw8=
=nHao
-----END PGP SIGNATURE-----

--------------w7atwMAiUaHQsKDKV5d0o0kr--"#;

pub const MULTIPART_MESSAGE_WITH_UNNAMED_ATTACHMENTS: &str = r#"From: Some One <someone@example.com>
To: "Someone Else" <someone-else@example.com>
MIME-Version: 1.0
Content-Type: multipart/mixed; boundary="XXXXboundary text"

This is a multipart message in MIME format.

--XXXXboundary text
Content-Type: text/plain

this is the body text

--XXXXboundary text
Content-Type: text/plain;
Content-Disposition: attachment;

this is the first attachment text

--XXXXboundary text
Content-Type: text/plain;
Content-Disposition: attachment;

this is the second attachment text

--XXXXboundary text--"#;

pub const MULTIPART_MESSAGE_WITH_ENCRYPTED_SUBJECT_UTF8: &str = r#"Content-Type: multipart/signed; micalg=pgp-sha256;
 protocol="application/pgp-signature";
 boundary="------------3mBgKY4DhzDe0cOovVcT4QQv"

This is an OpenPGP/MIME signed message (RFC 4880 and 3156)
--------------3mBgKY4DhzDe0cOovVcT4QQv
Content-Type: multipart/mixed; boundary="------------7VgK7B2dk0pUYjHBY0Zi2Fda";
 protected-headers="v1"
Subject: =?UTF-8?B?c3ViamVjdCB3aXRoIGVtb2ppcyDwn5iD8J+Yhw==?=
From: Sender <sender@example.com>
To: receiver@example.com
Message-ID: <7daafa18-8595-8065-3eba-b08c07becf36@example.com>

--------------7VgK7B2dk0pUYjHBY0Zi2Fda
Content-Type: multipart/mixed; boundary="------------D5jH01SvFZAwYShsjQamYW8w"

--------------D5jH01SvFZAwYShsjQamYW8w
Content-Type: text/plain; charset=UTF-8; format=flowed
Content-Transfer-Encoding: base64

dGVzdCB1dGY4IGluIGVuY3J5cHRlZCBzdWJqZWN0DQo=
--------------D5jH01SvFZAwYShsjQamYW8w
Content-Type: application/pgp-keys; name="OpenPGP_0xabc.asc"
Content-Disposition: attachment; filename="OpenPGP_0xabc.asc"
Content-Description: OpenPGP public key
Content-Transfer-Encoding: quoted-printable

-----BEGIN PGP PUBLIC KEY BLOCK-----

...
-----END PGP PUBLIC KEY BLOCK-----

--------------D5jH01SvFZAwYShsjQamYW8w--

--------------7VgK7B2dk0pUYjHBY0Zi2Fda--

--------------3mBgKY4DhzDe0cOovVcT4QQv
Content-Type: application/pgp-signature; name="OpenPGP_signature.asc"
Content-Description: OpenPGP digital signature
Content-Disposition: attachment; filename="OpenPGP_signature"

-----BEGIN PGP SIGNATURE-----

wnUEARYKAAYFAmIwlfMAIQkQdqGsuYvE1jgWIQRGvajOG9a8ZbdysiN2oay5
i8TWOBX5AP0V5H79/eiraXKKBCvpqwcEzrv1DHfhvrjTHk9L6PIadgD/fXdv
WTyjgksKkPV68HhW1CIKZ4JIMe726uldjP6tgw8=
=nHao
-----END PGP SIGNATURE-----

--------------3mBgKY4DhzDe0cOovVcT4QQv--
"#;

pub const MESSAGE_WITH_EMPTY_BODY: &str = r#"Content-Type: multipart/mixed; boundary="------------P7E1gxp6rCvfn0to5n3PZ2h0";
protected-headers="v1"
From: Sender <sender@test.com>
To: receiver@pm.me
Message-ID: <39b3134c-0fcd-4618-b1bd-2b20481bf2af>
Subject: Empty message test

--------------P7E1gxp6rCvfn0to5n3PZ2h0
Content-Type: text/plain; charset=UTF-8; format=flowed
Content-Transfer-Encoding: 7bit


--------------P7E1gxp6rCvfn0to5n3PZ2h0--"#;

const MULTIPART_MESSAGE_WITH_SPECIAL_CHARACTER: &str = r#"From: Jon Smith <jon@example.com>
To: Jon Smith <jon@example.com>
Mime-Version: 1.0
Content-Type: multipart/signed; boundary==-=pj+EhsWuSQJxx7=-=; micalg=pgp-md5;
protocol="application/pgp-signature"

--=-=pj+EhsWuSQJxx7=-=
Content-Type: text/plain; charset=iso-8859-1
Content-Transfer-Encoding: quoted-printable

hello
--=-=pj+EhsWuSQJxx7=-=
Content-Type: application/pgp-signature

-----BEGIN PGP SIGNATURE-----
Version: OpenPGP.js v4.4.6
Comment: https://openpgpjs.org

wl4EARYKAAYFAlxuurAACgkQApgazZlru7PubwEAkm2yNgMcCzv9YuW2zKEP
eo6TtHjWxF3GASwuZ/nMv/MBAJUDDC3PDfCIGyPKk2Pzf2t2co/+dEpW3vpx
euiL4uYD
=97+O
-----END PGP SIGNATURE-----

--=-=pj+EhsWuSQJxx7=-=
"#;

#[test]
fn test_decodes_utf8_body_with_8bit_transfer_encoding() {
    let input = r#"From: Some One <someone@example.com>
To: "Someone Else" <someone-else@example.com>
MIME-Version: 1.0
Content-Type: multipart/mixed;
    boundary="------------cJMvmFk1NneB7MT4jwYHY7ap"

This is a multi-part message in MIME format.
--------------cJMvmFk1NneB7MT4jwYHY7ap
Content-Type: text/plain; charset=UTF-8;
Content-Transfer-Encoding: 8bit

Import HTML cöntäct//Subjεέςτ//

--------------cJMvmFk1NneB7MT4jwYHY7ap--"#;
    let processed_message = MimeProcessor::process_mime("message_id", input.as_bytes()).unwrap();

    assert_eq!(&processed_message.body, "Import HTML cöntäct//Subjεέςτ//\n");
}

fn verify_signature(raw_input: &str, signatures: &[MimeSignatureVerifier]) -> VerificationResult {
    let provider = new_pgp_provider();
    let pk = provider
        .public_key_import(KEY, DataEncoding::Armor)
        .unwrap();
    let sig = signatures.first().unwrap();
    let body = to_canonicalized_string(sig.data_to_verify(raw_input.as_bytes()), true).unwrap();
    provider
        .new_verifier()
        .with_verification_key(&pk)
        .verify_detached(
            body.as_bytes(),
            sig.pgp_signature.as_bytes(),
            DataEncoding::Armor,
        )
}

#[test]
fn test_process_multipart_signed_mime_messages_and_verify_signature() {
    let processed_message =
        MimeProcessor::process_mime("message_id", MULTIPART_SIGNED_MESSAGE.as_bytes()).unwrap();

    assert_eq!(&processed_message.body, MULTIPART_SIGNED_MESSAGE_BODY);
    assert!(processed_message.attachments.is_empty());
    assert!(processed_message.encrypted_subject.is_none());
    assert!(!processed_message.signatures.is_empty());
    let verification_result =
        verify_signature(MULTIPART_SIGNED_MESSAGE, &processed_message.signatures);
    assert!(verification_result.is_ok());
}

#[test]
fn test_process_multipart_signed_mime_messages_and_verify_signature_with_extra_parts() {
    let processed_message =
        MimeProcessor::process_mime("message_id", EXTRA_MULTIPART_SIGNED_MESSAGE.as_bytes())
            .unwrap();

    assert_eq!(&processed_message.body, "hello");
    assert!(processed_message.attachments.is_empty());
    assert!(!processed_message.signatures.is_empty());
    let verification_result = verify_signature(
        EXTRA_MULTIPART_SIGNED_MESSAGE,
        &processed_message.signatures,
    );
    assert!(verification_result.is_ok());
}

#[test]
fn test_does_not_verify_invalid_messages() {
    let processed_message =
        MimeProcessor::process_mime("message_id", INVALID_MULTIPART_SIGNED_MESSAGE.as_bytes())
            .unwrap();

    assert_eq!(&processed_message.body, "message with missing signature");
    assert!(processed_message.signatures.is_empty());
}

#[test]
fn test_can_parse_messages_with_special_characters_in_boundary() {
    let processed_message = MimeProcessor::process_mime(
        "message_id",
        MULTIPART_MESSAGE_WITH_SPECIAL_CHARACTER.as_bytes(),
    )
    .unwrap();

    assert!(!processed_message.signatures.is_empty());
    assert_eq!(&processed_message.body, "hello");
    let verification_result = verify_signature(
        MULTIPART_MESSAGE_WITH_SPECIAL_CHARACTER,
        &processed_message.signatures,
    );
    assert!(verification_result.is_ok());
}

#[test]
fn test_can_parse_message_with_empty_body() {
    let processed_message =
        MimeProcessor::process_mime("message_id", MESSAGE_WITH_EMPTY_BODY.as_bytes()).unwrap();

    assert_eq!(processed_message.body, "");
}

#[test]
fn test_can_parse_message_with_text_attachment() {
    let processed_message =
        MimeProcessor::process_mime("message_id", MULTIPART_MESSAGE_WITH_ATTACHMENT.as_bytes())
            .unwrap();

    assert!(processed_message.signatures.is_empty());
    assert_eq!(&processed_message.body, "this is the body text\n");
    assert_eq!(processed_message.attachments.len(), 1);

    let attachment = processed_message.attachments.first().unwrap();
    assert_eq!(attachment.name, "test.txt");
    assert_eq!(attachment.mime_type, "text/plain");
    assert!(attachment.content_id.contains("pmcrypto"));
}

#[test]
fn test_can_parse_message_with_encrypted_subject() {
    let processed_message = MimeProcessor::process_mime(
        "message_id",
        MULTIPART_MESSAGE_WITH_ENCRYPTED_SUBJECT.as_bytes(),
    )
    .unwrap();
    // TODO: Encrypted subject not yet implemented in lower level.
    // assert_eq!(encrypted_subject, "Encrypted subject");
    assert_eq!(processed_message.signatures.len(), 1);
    assert_eq!(&processed_message.body, "hello");

    let verification_result = verify_signature(
        MULTIPART_MESSAGE_WITH_ENCRYPTED_SUBJECT,
        &processed_message.signatures,
    );
    matches!(verification_result, Err(VerificationError::Failed(_, _)));
}

#[test]
fn test_generates_different_filenames_for_multiple_attachments_with_empty_names() {
    let processed_message = MimeProcessor::process_mime(
        "message_id",
        MULTIPART_MESSAGE_WITH_UNNAMED_ATTACHMENTS.as_bytes(),
    )
    .unwrap();

    assert_eq!(processed_message.attachments.len(), 2);
    let attachment = processed_message.attachments.first().unwrap();
    assert_eq!(attachment.name, "attachment.txt");

    let attachment_other = processed_message.attachments.get(1).unwrap();
    assert_eq!(attachment_other.name, "attachment.txt (1)");
    assert_ne!(attachment.content_id, attachment_other.content_id);
}

#[test]
fn test_can_parse_message_with_encrypted_subject_containing_non_ascii_chars() {
    let processed_message = MimeProcessor::process_mime(
        "message_id",
        MULTIPART_MESSAGE_WITH_ENCRYPTED_SUBJECT_UTF8.as_bytes(),
    )
    .unwrap();
    // TODO: Encrypted subject not yet implemented in lower level.
    //assert_eq!(&processed_message.encrypted_subject.unwrap_or(String::new()), "subject with emojis 😃😇");
    assert_eq!(&processed_message.body, "test utf8 in encrypted subject\n");
}
