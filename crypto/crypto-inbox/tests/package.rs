use std::iter;

use proton_crypto_inbox::{
    message::packages::{EncryptablePackage, PackageMimeType},
    proton_crypto::crypto::{
        DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerifiedData,
    },
};

mod common;
use common::{create_account_unlocked_address_key, TEST_DECRYPTION_KEY};

const RECIPIENT_ONE: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

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

const RECIPIENT_TWO: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

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

const PLAINTEXT: &str = "<b>Hello World</b>      \n";
const PLAINTEXT_EXPECTED: &str = "<b>Hello World</b>\r\n";

struct TestPlainPackage {
    mime_type: PackageMimeType,
    content: Vec<u8>,
}

impl Default for TestPlainPackage {
    fn default() -> Self {
        Self {
            mime_type: PackageMimeType::Html,
            content: PLAINTEXT.into(),
        }
    }
}

impl EncryptablePackage for TestPlainPackage {
    fn package_mime_type(&self) -> PackageMimeType {
        self.mime_type
    }

    fn package_body_content(&self) -> &[u8] {
        &self.content
    }
}

fn create_test_recipient_keys<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
) -> (Vec<Provider::PrivateKey>, Vec<Provider::PublicKey>) {
    let r1 = pgp_provider
        .private_key_import(
            RECIPIENT_ONE.as_bytes(),
            "password".as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();
    let r1_pub = pgp_provider.private_key_to_public_key(&r1).unwrap();
    let r2 = pgp_provider
        .private_key_import(
            RECIPIENT_TWO.as_bytes(),
            "password".as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();
    let r2_pub = pgp_provider.private_key_to_public_key(&r2).unwrap();
    (vec![r1, r2], vec![r1_pub, r2_pub])
}

#[test]
fn test_package_create() {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let plain_package = TestPlainPackage::default();
    let address_key =
        create_account_unlocked_address_key(&pgp_provider, TEST_DECRYPTION_KEY, "password");
    let encrypted_package = plain_package
        .package_body_encrypt(&pgp_provider, &address_key)
        .expect("should encrypt");

    // Package should decrypt with session key.
    let data = pgp_provider
        .new_decryptor()
        .with_session_key(
            encrypted_package
                .session_key
                .export_to_pgp_provider(&pgp_provider)
                .unwrap(),
        )
        .decrypt(&encrypted_package.encrypted_body, DataEncoding::Bytes)
        .expect("package should decrypt with session key");
    assert_eq!(data.as_bytes(), PLAINTEXT_EXPECTED.as_bytes());
}

#[test]
fn test_package_with_key_packets_create() {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let plain_package = TestPlainPackage::default();

    let address_key =
        create_account_unlocked_address_key(&pgp_provider, TEST_DECRYPTION_KEY, "password");
    let encrypted_package = plain_package
        .package_body_encrypt(&pgp_provider, &address_key)
        .expect("should encrypt");

    let (recipients_priv, recipients_priv_pub) = create_test_recipient_keys(&pgp_provider);
    let key_packets = encrypted_package
        .session_key
        .encrypt_to_recipients(&pgp_provider, &recipients_priv_pub)
        .expect("key packet create must succeed");
    for (key_packet, recipient_key) in key_packets.iter().zip(recipients_priv.iter()) {
        let mut message = key_packet.decode().expect("decode must succeed");
        message.extend(encrypted_package.encrypted_body.iter());
        let dec_result = pgp_provider
            .new_decryptor()
            .with_decryption_key(recipient_key)
            .with_verification_key(&address_key.public_key)
            .decrypt(&message, DataEncoding::Bytes)
            .expect("decryption must succeed");
        assert_eq!(dec_result.as_bytes(), PLAINTEXT_EXPECTED.as_bytes());
        assert!(dec_result.verification_result().is_ok());
    }
}

#[test]
fn test_package_create_mime_large_compression() {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let plain_package = TestPlainPackage {
        mime_type: PackageMimeType::Multipart,
        content: iter::repeat(1).take(1024 * 1024 + 1).collect(),
    };
    let address_key =
        create_account_unlocked_address_key(&pgp_provider, TEST_DECRYPTION_KEY, "password");
    let encrypted_package = plain_package
        .package_body_encrypt(&pgp_provider, &address_key)
        .expect("should encrypt");
    assert!(plain_package.content.len() > encrypted_package.encrypted_body.len());
}
