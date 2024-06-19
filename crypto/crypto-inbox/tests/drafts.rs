use proton_crypto_inbox::message::DecryptableMessage;
use proton_crypto_inbox::proton_crypto::crypto::{DataEncoding, PGPProviderSync};
use proton_crypto_inbox::{message::DraftEncryption, proton_crypto::new_pgp_provider};

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

struct TestDraft(Vec<u8>);

impl DraftEncryption for TestDraft {
    fn message_body(&self) -> &[u8] {
        &self.0
    }
}

struct TestMessage(pub bool, pub String);

impl DecryptableMessage for TestMessage {
    fn message_is_mime(&self) -> bool {
        self.0
    }

    fn message_encrypted_body(&self) -> &[u8] {
        self.1.as_bytes()
    }

    fn message_id(&self) -> Option<&str> {
        Some("unique-message-id")
    }
}

#[test]
fn test_encrypt_and_decrypt_draft() {
    let pgp_provider = new_pgp_provider();
    let private_key = pgp_provider
        .private_key_import(
            PRIVATE_KEY.as_bytes(),
            "password".as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();
    let message = "hello_world";
    let draft = TestDraft(message.as_bytes().to_owned());

    let encrypted_draft =
        String::from_utf8(draft.encrypt_draft(&pgp_provider, &private_key).unwrap()).unwrap();

    let decryptable_message = TestMessage(false, encrypted_draft);

    let plain_text = decryptable_message
        .decrypt(&pgp_provider, &[private_key])
        .unwrap();

    assert_eq!(plain_text.0.body(), message);
}
