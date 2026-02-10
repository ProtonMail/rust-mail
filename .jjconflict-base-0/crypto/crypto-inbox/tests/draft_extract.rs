use core::str;

use base64::{Engine as _, prelude::BASE64_STANDARD as BASE_64};
use proton_crypto_inbox::message::{GettablePGPMessage, SessionKeyAndDataPacketsExtractable};
use proton_crypto_inbox::proton_crypto::crypto::{
    DataEncoding, Decryptor, DecryptorSync, Encryptor, EncryptorSync, PGPProviderSync,
    SessionKeyAlgorithm, VerifiedData,
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

struct TestEncryptedDraft(pub String);

impl GettablePGPMessage for TestEncryptedDraft {
    fn pgp_message(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl SessionKeyAndDataPacketsExtractable for TestEncryptedDraft {}

#[test]
fn test_extract_keys_and_data_from_draft() {
    let pgp = new_pgp_provider();

    let private_key = pgp
        .private_key_import(
            PRIVATE_KEY.as_bytes(),
            "password".as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();

    let public_key = pgp.private_key_to_public_key(&private_key).unwrap();
    let message = "hello_world";

    let session_key = pgp
        .session_key_import(
            "Never gonna give/let you up/down".as_bytes(),
            SessionKeyAlgorithm::Aes256,
        )
        .expect("Rick wouldn't fail us");

    let draft_data = pgp
        .new_encryptor()
        .with_session_key(session_key)
        .with_encryption_key(&public_key)
        .with_signing_key(&private_key)
        .encrypt_raw(message, DataEncoding::Armor)
        .expect("creating the encrypted draft must not fail");

    let encrypted_draft = TestEncryptedDraft(
        String::from_utf8(draft_data).expect("encoding to string should not fail"),
    );

    let (session_key, data_packets) = encrypted_draft
        .extract_session_key_and_data_packets(&pgp, &[&private_key])
        .expect("extracting packets should not fail");

    assert_eq!(
        String::from_utf8(
            BASE_64
                .decode(session_key.expose_secret().as_bytes())
                .unwrap()
        )
        .expect("string conversion should not fail"),
        "Never gonna give/let you up/down"
    );

    let session_key_provider = session_key
        .export_to_pgp_provider(&pgp)
        .expect("Failed to export session key to provider");

    let decrypted_data = pgp
        .new_decryptor()
        .with_session_key(session_key_provider)
        .decrypt(data_packets.as_ref(), DataEncoding::Bytes)
        .expect("decryption is not expected to fail");

    let decrypted_data = decrypted_data.as_bytes();

    assert_eq!(decrypted_data, message.as_bytes());
}
