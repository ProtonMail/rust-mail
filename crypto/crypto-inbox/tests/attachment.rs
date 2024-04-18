use std::io::{self, Write};

use base64::Engine;
use proton_crypto_account::proton_crypto::crypto::{AsPublicKeyRef, PrivateKey, PublicKey};
use proton_crypto_inbox::attachment::{
    encrypt, encrypt_and_sign_to_writer, AttachmentDecryption, AttachmentEncryptedSignature,
    AttachmentSignature, KeyPackets,
};

use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;

mod common;
use common::{get_test_address_keys, get_test_public_address_keys};

const TEST_ATTACHMENT_METADATA_KP: &str = "wV4Di5gBfuEszfESAQdAUGm56qPuhgLjuStIEcL07fKh10ptOYc0UnB2kTwqqhMw2ivOpsuDSOM17OPsxG35znCodjKBxM1O+DeFuYhel8TsuJjNxKltBgv/jVs48LGw";

const TEST_ATTACHMENT_METADATA_SIG: &str = "-----BEGIN PGP SIGNATURE-----
Version: ProtonMail

wnUEABYKACcFgmXfEFQJkFX2DKhfS5UBFiEESUD3387U/LDImeGNVfYMqF9L
lQEAAFdZAQC8eHZNqU3wS/4YVktAE2JYHwUevloBCSiR+ACiF4y6vgD9EcZN
t5Wf5KU9FQ3zqhrBIqeaLDLhnox+Yyq/K7U8mgs=
=/5GZ
-----END PGP SIGNATURE-----
";

const TEST_ATTACHMENT_METADATA_ENC_SIG: &str = "-----BEGIN PGP MESSAGE-----
Version: ProtonMail

wV4Di5gBfuEszfESAQdAhEdKb/7Gvp/iz/tCs3+rmSW93ySpnCUoizzGDfUs
zUIwwJ3V8I+Mm7Y0L1Tw9uyLzkOWjQMzyRteFkIpMZKK0+ZjukxQIsmgheC3
9sE51xvd0qgB6U1djOrlhXcSu4ufZ6NSpFM/T1JKZe7EVu2kXPTKv0veqlfh
P/VM6YWaNGugaPzvZcchQQC5tRhxogVmbDrSUirJYnNa9z/qEF6FcBpOXc59
3w6S5zRMD3bWEA53PVNFQBHAVdBFIkKW14/QIQ26lZM295VLu1WUXPX9eso4
EiWuw4/+aNQICAeabHV26Mtsp/DI6AZ7DtjMdNxDOFFeQ5Col6Ofu8E=
=pQ9a
-----END PGP MESSAGE-----";

const TEST_ATTACHMENT_ENC_DATA: &str =
    "0kABGVu3HPPyl7wHJhXxg7+E69aHqqYR2cPcDn5Fai0jb2K/1fC8rqzo5jKxF4yca3CK5PRmLz4F9S2GobFvgmtv";

const TEST_ATTACHMENT_PLAIN_DATA: &str = "test attachment";

struct TestAttachmentMetdata {
    key_packets: KeyPackets,
    signature: Option<AttachmentSignature>,
    enc_signature: Option<AttachmentEncryptedSignature>,
}

struct TestAddressKey<T: PrivateKey>(T);

impl<T: PrivateKey> AsRef<T> for TestAddressKey<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}
struct TestAddressPublicKey<T: PublicKey>(T);

impl<T: PublicKey> AsPublicKeyRef<T> for TestAddressPublicKey<T> {
    fn as_public_key(&self) -> &T {
        &self.0
    }
}

impl AttachmentDecryption for TestAttachmentMetdata {
    fn attachment_key_packets(&self) -> &KeyPackets {
        &self.key_packets
    }

    fn attachment_signature(&self) -> &Option<AttachmentSignature> {
        &self.signature
    }

    fn attachment_encrypted_signature(&self) -> &Option<AttachmentEncryptedSignature> {
        &self.enc_signature
    }
}

fn get_test_attachment_metadata() -> TestAttachmentMetdata {
    TestAttachmentMetdata {
        key_packets: KeyPackets::from(TEST_ATTACHMENT_METADATA_KP),
        signature: Some(AttachmentSignature::from(TEST_ATTACHMENT_METADATA_SIG)),
        enc_signature: Some(AttachmentEncryptedSignature::from(
            TEST_ATTACHMENT_METADATA_ENC_SIG,
        )),
    }
}

fn get_test_attachment_metadata_enc_sig_only() -> TestAttachmentMetdata {
    TestAttachmentMetdata {
        key_packets: KeyPackets::from(TEST_ATTACHMENT_METADATA_KP),
        signature: None,
        enc_signature: Some(AttachmentEncryptedSignature::from(
            TEST_ATTACHMENT_METADATA_ENC_SIG,
        )),
    }
}

fn get_test_attachment_encrypted_data() -> Vec<u8> {
    let b64 = base64::engine::general_purpose::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::general_purpose::PAD,
    );
    b64.decode(TEST_ATTACHMENT_ENC_DATA).unwrap()
}

fn test_attachment_decrypt_helper(attachment_metadata: &impl AttachmentDecryption) {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let decryption_keys = get_test_address_keys(&pgp_provider);
    let verification_keys = get_test_public_address_keys(&pgp_provider);

    let enc_data: Vec<u8> = get_test_attachment_encrypted_data();
    let decrypted_attachment = attachment_metadata
        .decrypt(
            &pgp_provider,
            &decryption_keys,
            &verification_keys,
            enc_data,
        )
        .unwrap();

    assert_eq!(
        decrypted_attachment.as_ref(),
        TEST_ATTACHMENT_PLAIN_DATA.as_bytes()
    );

    let verification_result = decrypted_attachment.verification_result();
    assert!(verification_result.is_ok());
}

fn test_attachment_decrypt_stream_helper(attachment_metadata: &impl AttachmentDecryption) {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let decryption_keys = get_test_address_keys(&pgp_provider);
    let verification_keys = get_test_public_address_keys(&pgp_provider);

    let enc_data: Vec<u8> = get_test_attachment_encrypted_data();
    let mut output_buffer = Vec::new();
    let enc_data_reader: &[u8] = enc_data.as_ref();
    let mut verification_reader = attachment_metadata
        .decrypt_from_reader(
            &pgp_provider,
            &decryption_keys,
            &verification_keys,
            enc_data_reader,
        )
        .unwrap();
    io::copy(&mut verification_reader, &mut output_buffer).unwrap();
    assert_eq!(&output_buffer, TEST_ATTACHMENT_PLAIN_DATA.as_bytes());

    let verification_result = verification_reader.verification_result();
    assert!(verification_result.is_ok());
}

#[test]
fn test_attachment_decrypt() {
    let attachment_metadata = get_test_attachment_metadata();
    test_attachment_decrypt_helper(&attachment_metadata);
}

#[test]
fn test_attachment_decrypt_encrypted_signature() {
    let attachment_metadata = get_test_attachment_metadata_enc_sig_only();
    test_attachment_decrypt_helper(&attachment_metadata);
}

#[test]
fn test_attachment_decrypt_stream() {
    let attachment_metadata = get_test_attachment_metadata();
    test_attachment_decrypt_stream_helper(&attachment_metadata);
}

#[test]
fn test_attachment_decrypt_stream_encrypted_signature() {
    let attachment_metadata = get_test_attachment_metadata_enc_sig_only();
    test_attachment_decrypt_stream_helper(&attachment_metadata);
}

fn test_attachment_encrypt_decrypt_helper(enc_sig: bool) {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let priv_keys = get_test_address_keys(&pgp_provider);
    let pub_keys: Vec<_> = priv_keys
        .iter()
        .map(|key| {
            pgp_provider
                .private_key_to_public_key(key.as_ref())
                .unwrap()
        })
        .collect();

    let mut result = encrypt(
        &pgp_provider,
        &pub_keys,
        &priv_keys,
        TEST_ATTACHMENT_PLAIN_DATA,
    )
    .unwrap();

    if enc_sig {
        result.metadata.signature = None;
    }

    // Sig should be ok
    let decrypted_attachment = result
        .metadata
        .decrypt(&pgp_provider, &priv_keys, &pub_keys, &result.data)
        .unwrap();

    assert_eq!(
        decrypted_attachment.as_ref(),
        TEST_ATTACHMENT_PLAIN_DATA.as_bytes()
    );

    let verification_result = decrypted_attachment.verification_result();
    assert!(verification_result.is_ok());

    // Sig should be not ok
    let wrong_keys = get_test_public_address_keys(&pgp_provider);
    let decrypted_attachment_wrong = result
        .metadata
        .decrypt(&pgp_provider, &priv_keys, &wrong_keys, &result.data)
        .unwrap();

    let verification_result = decrypted_attachment_wrong.verification_result();
    assert!(verification_result.is_err());
}

#[test]
fn test_attachment_encrypt_decrypt() {
    test_attachment_encrypt_decrypt_helper(false);
}

#[test]
fn test_attachment_encrypt_decrypt_encrypted_signature() {
    test_attachment_encrypt_decrypt_helper(true);
}

fn test_attachment_encrypt_decrypt_stream_helper(enc_sig: bool) {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let priv_keys = get_test_address_keys(&pgp_provider);
    let pub_keys: Vec<_> = priv_keys
        .iter()
        .map(|key| {
            pgp_provider
                .private_key_to_public_key(key.as_ref())
                .unwrap()
        })
        .collect();
    let mut data = Vec::with_capacity(TEST_ATTACHMENT_PLAIN_DATA.len());
    let mut metadata = {
        let mut attachment_writer =
            encrypt_and_sign_to_writer(&pgp_provider, &pub_keys, &priv_keys, &mut data).unwrap();
        attachment_writer
            .write_all(TEST_ATTACHMENT_PLAIN_DATA.as_bytes())
            .unwrap();
        attachment_writer.finalize().unwrap()
    };

    if enc_sig {
        metadata.signature = None;
    }

    // Sig should be ok
    let decrypted_attachment = metadata
        .decrypt(&pgp_provider, &priv_keys, &pub_keys, &data)
        .unwrap();

    assert_eq!(
        decrypted_attachment.as_ref(),
        TEST_ATTACHMENT_PLAIN_DATA.as_bytes()
    );

    let verification_result = decrypted_attachment.verification_result();
    assert!(verification_result.is_ok());

    // Sig should be not ok
    let wrong_keys = get_test_public_address_keys(&pgp_provider);
    let decrypted_attachment_wrong = metadata
        .decrypt(&pgp_provider, &priv_keys, &wrong_keys, &data)
        .unwrap();

    let verification_result = decrypted_attachment_wrong.verification_result();
    assert!(verification_result.is_err());
}

#[test]
fn test_attachment_encrypt_decrypt_stream() {
    test_attachment_encrypt_decrypt_stream_helper(false);
}

#[test]
fn test_attachment_encrypt_decrypt_encrypted_signature_stream() {
    test_attachment_encrypt_decrypt_stream_helper(true);
}
