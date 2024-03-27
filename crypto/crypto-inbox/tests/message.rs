use proton_crypto::crypto::VerificationError;
use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody};

mod common;
use common::{get_test_address_keys, get_test_public_address_keys};

use crate::common::{get_test_address_key_source, get_test_public_address_key_source};

pub const TEST_VERIFICATION_KEY_MIME: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----

xjMEZf15lRYJKwYBBAHaRw8BAQdArPz06hKiOUYSVs6dbHpKSh63bW5/QyIF
qRvJ5wOALJnNMkx1a2FzIEJ1cmtoYWx0ZXIgPGtleXRyYW5zcGFyZW5jeW1h
aWxlckBnbWFpbC5jb20+wo8EExYIADcWIQSNEf53FU6EMmZs43pG8PpwjTNi
IAUCZf15lQUJBaOagAIbAwQLCQgHBRUICQoLBRYCAwEAAAoJEEbw+nCNM2Ig
aX0BANKGrENgM7nbpt5uORfaT5JLx695q1RgKDetm6bQhB1/AQDHvY3oha+e
abN+yKcOWKlvvNpbbbYzjunnrmfm7d+HDM44BGX9eZUSCisGAQQBl1UBBQEB
B0Aq4KRFu4d/XmR2UEGjsXeWCWvvKUkzsCR/wRDn8E/lRQMBCAfCfgQYFggA
JhYhBI0R/ncVToQyZmzjekbw+nCNM2IgBQJl/XmVBQkFo5qAAhsMAAoJEEbw
+nCNM2IgEzcBAPqEmyOcnbzbsGJaZ5uFEA3OfGH7anEg2xEbfZ0jxAh0AP9n
sO+JqQrVW5m3aGW4MRMFRjnC2DIHthThNQMw1bZpDQ==
=ziuc
-----END PGP PUBLIC KEY BLOCK-----
";

pub const TEST_DECRYPTION_KEY_MIME: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZSfovhYJKwYBBAHaRw8BAQdA6gS5mfVImh6ONhKgZGSVrLH4cdZaS9IW
6FhqYGWe2wr+CQMI7cZcc+SQB+tgAAAAAAAAAAAAAAAAAAAAAKEiVaK2iq+g
Y3+lmnRmmRZ4/HeC9UOoRmmFxHiHqFflv+bfqRD3hL2/+ayIG4MpahvRrnd0
ss0nbHVidXhAcHJvdG9uLmJsYWNrIDxsdWJ1eEBwcm90b24uYmxhY2s+wowE
EBYKAD4FgmUn6L4ECwkHCAmQVfYMqF9LlQEDFQgKBBYAAgECGQECmwMCHgEW
IQRJQPffztT8sMiZ4Y1V9gyoX0uVAQAAIOMA+wUpEGAm8SsDMt/tuaTSYrV/
DBsUzTYtFbzoBkT+dOLRAQDvZ4Z/YUn7mX71v0qXVTfGY5oLnY88Wuo9dySU
ns8kB8eLBGUn6L4SCisGAQQBl1UBBQEBB0DzvEDbVNT8WhIxijPVGHKGQ1Y3
s9Zw1i63nkkSnpLzNwMBCAf+CQMICODa4UCuLdlgAAAAAAAAAAAAAAAAAAAA
ABF+V4UBANv2UoEWSWPt2lltQkXnsXZ9rB5NkywVQwqc5vW/h3yx5vjZEY10
4jA3eSBo2bIaocJ4BBgWCAAqBYJlJ+i+CZBV9gyoX0uVAQKbDBYhBElA99/O
1PywyJnhjVX2DKhfS5UBAAASqQEA4qisiR8EHC6S7/EsUhS2uuin1tY0KQ0j
1jmrk+HHQugA/in2lPCiO/6RdSLXnbXnGj+7lP65+qrMXHb+mqBRdWsA
-----END PGP PRIVATE KEY BLOCK-----
";

const TEST_MESSAGE_BODY: &str = "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4Di5gBfuEszfESAQdAzv+eAfvm7tTd8GHvGn3Qsp2LhI2yjtKgSeg7uS69\nDV0w3DaikcJRSBvqQPWkkimzIdpyBIe4fzIaVERcUil0PTd+F8/zljGWTfNj\n29c030K90sBdATjoTBKarkG1Th7sllv1mC51vuxlvFateZmiLDNDeog6SdwM\n0YI9eKyT2+Wpyi9ehfw6HAwlMKDMY0ybFxhBCSpuWSZ9kIenGKJMym3MhkJM\nJu4J4F+PcZwO+katTJN4CnqyrGSOJYllECWqggZDdoF4nEm3G2LYI1W573Q6\no+fRqywqyPdHaqDiqviuL29RsqeG+Y+4TxQhXS2i4AfbhkBw1pv0fudTlNCu\nBSerK9SkpBKeDRxbfmmaRVPL0aFZjjwFYy0USg0JP0VEWClB0CCLiKhHvQsE\nUSy5VGT9ChsTRl2idtc2iUcfBUKiLT8JlAFfzFVW8WZgfpEEmUgSNS06/SQ/\ncaz1Mm9EF6xfkiBjxwDG7iEZSHIbzMCi\n=7AjW\n-----END PGP MESSAGE-----\n";
const TEST_EXPECTED_BODY: &str = r#"<div style="font-family: Arial, sans-serif; font-size: 14px;"><span>Test Attachment</span><br></div>
"#;

const TEST_MESSAGE_MIME: &str = "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\n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\nVRx0RtmAQbLF/84EXzQJZxKg0mmnCg==\n=oKh2\n-----END PGP MESSAGE-----\n";

const TEST_EXPECTED_BODY_MIME: &str = r#"<!DOCTYPE html>
<html>
  <head>

    <meta http-equiv="content-type" content="text/html; charset=UTF-8">
  </head>
  <body>
    <p>This is a test <b>mime message from thunderbrid</b> with three
      attachments.<br>
    </p>
  </body>
</html>
"#;

struct TestMessage(pub bool, pub String);

impl DecryptableMessage for TestMessage {
    fn message_is_mime(&self) -> bool {
        self.0
    }

    fn message_encrypted_body(&self) -> &[u8] {
        self.1.as_bytes()
    }

    fn message_id(&self) -> &str {
        "unique-message-id"
    }
}

#[test]
fn test_message_decrypt_and_verify() {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let decryption_keys = get_test_address_keys(&pgp_provider);
    let mut verification_keys = get_test_public_address_keys(&pgp_provider);
    let test_message = TestMessage(false, TEST_MESSAGE_BODY.into());
    let (decrypted_message, verifier) = test_message
        .decrypt(&pgp_provider, &decryption_keys)
        .unwrap();
    assert_eq!(decrypted_message.as_ref(), TEST_EXPECTED_BODY);
    let verification_result = verifier.verify_signature(&pgp_provider, &verification_keys);
    assert!(verification_result.is_ok());
    verification_keys.remove(0);
    let verification_result_no_verifier =
        verifier.verify_signature(&pgp_provider, &verification_keys);
    assert!(matches!(
        verification_result_no_verifier.unwrap_err(),
        VerificationError::NoVerifier(_)
    ));
}

#[test]
fn test_message_decrypt_and_verify_mime() {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let decryption_keys = get_test_address_key_source(&pgp_provider, TEST_DECRYPTION_KEY_MIME);
    let verification_keys =
        get_test_public_address_key_source(&pgp_provider, TEST_VERIFICATION_KEY_MIME);

    let test_message = TestMessage(true, TEST_MESSAGE_MIME.into());
    let (decrypted_message, verifier) = test_message
        .decrypt(&pgp_provider, &decryption_keys)
        .unwrap();

    assert_eq!(decrypted_message.body(), TEST_EXPECTED_BODY_MIME);
    let verification_result = verifier.verify_signature(&pgp_provider, &verification_keys);
    assert!(verification_result.is_ok());

    assert!(decrypted_message.is_mime());
    let DecryptedBody::Mime(processed_messsage) = decrypted_message else {
        panic!("Must be a mime body");
    };

    assert_eq!(processed_messsage.encrypted_subject.unwrap(), "test mime");

    assert_eq!(processed_messsage.attachments.len(), 4);
    for (idx, attachment) in processed_messsage.attachments.iter().enumerate() {
        if idx != processed_messsage.attachments.len() - 1 {
            let expected_content = format!("attachment{}", idx + 1);
            let expected_name = format!("{}.txt", expected_content);
            assert_eq!(attachment.name, expected_name);
            assert_eq!(
                String::from_utf8(attachment.data.to_vec()).unwrap(),
                expected_content
            );
        }
    }

    let last_attachment = processed_messsage.attachments.last().unwrap();
    assert_eq!(last_attachment.name, "OpenPGP_0x46F0FA708D336220.asc");
}
