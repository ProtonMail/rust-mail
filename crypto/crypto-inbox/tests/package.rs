use std::iter;

use proton_crypto_inbox::{
    message::packages::{EncryptablePackage, PackageMimeType},
    proton_crypto::crypto::{
        DataEncoding, Decryptor, DecryptorSync, PGPProviderSync, VerifiedData,
    },
};

mod common;
use common::{
    create_account_unlocked_address_key, create_test_recipient_keys, TEST_DECRYPTION_KEY,
};

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
        message.extend(encrypted_package.encrypted_body.as_ref().iter());
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
    assert!(plain_package.content.len() > encrypted_package.encrypted_body.as_ref().len());
}
