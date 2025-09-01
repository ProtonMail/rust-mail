use proton_crypto_inbox::{
    message::{DecryptableMessage, EncryptableDraft, GettablePGPMessage},
    proton_crypto::crypto::{AccessKeyInfo, DataEncoding, PGPProviderSync},
};

mod common;
use common::{
    TEST_DECRYPTION_KEY_V6, create_account_unlocked_address_keys,
    create_account_unlocked_address_keys_v6,
};

use proton_crypto_inbox::proton_crypto::new_pgp_provider;

const PRIVATE_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZkI9XRYJKwYBBAHaRw8BAQdAqWn9T/nEzBz31DqsXAUdtIGUnrIHNrfD
ZOuvtEIf8G/+CQMI8okkwdBKjuxgKBsLTTZH6cHgAlro1OnVNykZFiG6qASZ
Omwsl9FjZdozMlq/4AyPwDG2tkwzyEHKzn+4/Daw+FBs9ve/2Z2kOq9aEn6I
nc07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp
bF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZkI9XQQLCQcICZBICuUpsYEG
ZQMVCAoEFgACAQIZAQKbAwIeARYhBCF533Bw13ukWKtNIUgK5SmxgQZlAADY
pAD+NXPfGC0v116+xHi9HIcdDUXrQpd8pRbKRcHHKZ94DF0BAOYorJa1OHzk
wzjxWQEz5Y82SLRYLNmlIVF+hHXlLtYAx4sEZkI9XRIKKwYBBAGXVQEFAQEH
QKm+vMMpfMo45etkNw3LR+jFgMrbe4hZ9zPVZCxJUZtRAwEIB/4JAwg+PXvg
gHJFA2ApL3+4HL9kuK3+HzOdTrWAGQ2dET1V4aV84gxW25FBTZbb+QqLhIym
+sYefVPntt6M/VNupYyXRs27abjPqm2YtH+rLnhwwngEGBYKACoFgmZCPV0J
kEgK5SmxgQZlApsMFiEEIXnfcHDXe6RYq00hSArlKbGBBmUAANN3AP9s1V6S
Lg9ogdrlmtTwuZhRXHQzUqC2AoLCv/lW0hz0ZQD+LBAeT7ymSyrwRtIvUh0b
qnK1SNfocPNfh//OecgiqA4=
=LMjL
-----END PGP PRIVATE KEY BLOCK-----
";

pub const DIFFERENT_PRIVATE_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

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

struct TestDraft(Vec<u8>);

impl EncryptableDraft for TestDraft {
    fn plaintext_message_body(&self) -> &[u8] {
        &self.0
    }
}

struct TestMessage(pub bool, pub String);

impl GettablePGPMessage for TestMessage {
    fn pgp_message(&self) -> &[u8] {
        self.1.as_bytes()
    }
}

impl DecryptableMessage for TestMessage {
    fn message_is_mime(&self) -> bool {
        self.0
    }

    fn message_id(&self) -> Option<&str> {
        Some("unique-message-id")
    }
}

#[test]
fn test_encrypt_and_decrypt_draft() {
    let pgp = new_pgp_provider();
    let message = "hello_world";
    let draft = TestDraft(message.as_bytes().to_owned());

    let unlocked_address_keys = create_account_unlocked_address_keys(&pgp, PRIVATE_KEY, "password");
    let primary = unlocked_address_keys
        .primary_for_mail()
        .expect("There should be a primary key");
    let encrypted_draft = draft
        .encrypt_draft_body(&pgp, &primary)
        .expect("encryption to succeed ");

    let decryptable_message = TestMessage(false, encrypted_draft.0);

    let plain_text = decryptable_message
        .decrypt(&pgp, &unlocked_address_keys)
        .unwrap();

    assert_eq!(plain_text.0.body(), message);
}

#[test]
fn test_encrypt_and_decrypt_draft_v6() {
    let pgp = new_pgp_provider();
    let message = "hello_world";
    let draft = TestDraft(message.as_bytes().to_owned());

    let unlocked_address_keys = create_account_unlocked_address_keys_v6(
        &pgp,
        PRIVATE_KEY,
        TEST_DECRYPTION_KEY_V6,
        "password",
    );
    let primary = unlocked_address_keys
        .primary_for_mail()
        .expect("There should be a primary key");

    assert!(
        primary.is_v6 && primary.for_encryption().version() == 6,
        "Primary must be v6"
    );

    let encrypted_draft = draft
        .encrypt_draft_body(&pgp, &primary)
        .expect("encryption to succeed ");

    let decryptable_message = TestMessage(false, encrypted_draft.0);

    let plain_text = decryptable_message
        .decrypt(&pgp, &unlocked_address_keys)
        .unwrap();
    let primary_key_v4 = unlocked_address_keys.primary_default().unwrap();
    let signature_verification_v4 = plain_text.1.verify_signature(&pgp, &[primary_key_v4]);

    assert_eq!(plain_text.0.body(), message);
    assert!(
        signature_verification_v4.is_ok(),
        "signature should verify with a v4 key"
    );
}

#[test]
fn test_draft_decryption_fails_if_wrong_key_used() {
    let pgp = new_pgp_provider();
    let message = "hello_world";
    let draft = TestDraft(message.as_bytes().to_owned());

    let unlocked_address_keys = create_account_unlocked_address_keys(&pgp, PRIVATE_KEY, "password");
    let primary = unlocked_address_keys
        .primary_for_mail()
        .expect("There should be a primary key");

    let encrypted_draft = draft
        .encrypt_draft_body(&pgp, &primary)
        .expect("encryption to succeed ");

    let decryptable_message = TestMessage(false, encrypted_draft.0);

    let private_key = pgp
        .private_key_import(
            DIFFERENT_PRIVATE_KEY.as_bytes(),
            "password".as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();

    let decryption_result = decryptable_message.decrypt(&pgp, &[private_key]);

    assert!(decryption_result.is_err());
}
