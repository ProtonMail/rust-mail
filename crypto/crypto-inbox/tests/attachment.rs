use std::io::{self, Write};

use base64::Engine;
use proton_crypto_inbox::{
    attachment::{
        AttachmentEncryptedSignature, AttachmentSignature, DecryptableAttachment,
        EncryptableAttachment, EncryptedAttachmentMetadata, KeyPackets, encrypt_and_sign_to_writer,
    },
    proton_crypto::crypto::{Decryptor, DecryptorSync, PGPProviderSync, SessionKey},
};

mod common;
use common::{
    TEST_DECRYPTION_KEY, TEST_DECRYPTION_KEY_V6, create_account_unlocked_address_keys,
    create_account_unlocked_address_keys_v6, create_test_recipient_keys, get_test_address_keys,
    get_test_public_address_keys,
};

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

impl DecryptableAttachment for TestAttachmentMetdata {
    fn attachment_key_packets(&self) -> &KeyPackets {
        &self.key_packets
    }

    fn attachment_signature(&self) -> Option<&AttachmentSignature> {
        self.signature.as_ref()
    }

    fn attachment_encrypted_signature(&self) -> Option<&AttachmentEncryptedSignature> {
        self.enc_signature.as_ref()
    }
}

// Wrapper type for the tests.
struct DecryptableAttachmentMetadata {
    key_packets: KeyPackets,
    armored_signature: Option<AttachmentSignature>,
    armored_encrypted_signature: Option<AttachmentEncryptedSignature>,
}

impl DecryptableAttachmentMetadata {
    pub fn new<P>(pgp: &P, original: &EncryptedAttachmentMetadata) -> Self
    where
        P: PGPProviderSync,
    {
        let armored_signature = original
            .signature
            .as_ref()
            .map(|data| data.armor(pgp).expect("failed to armor"));

        let armored_encrypted_signature = original
            .encrypted_signature
            .as_ref()
            .map(|data| data.armor(pgp).expect("failed to armor"));

        Self {
            key_packets: KeyPackets::new_from_bytes(&original.key_packets),
            armored_signature,
            armored_encrypted_signature,
        }
    }
}

impl DecryptableAttachment for DecryptableAttachmentMetadata {
    fn attachment_key_packets(&self) -> &KeyPackets {
        &self.key_packets
    }

    fn attachment_signature(&self) -> Option<&AttachmentSignature> {
        self.armored_signature.as_ref()
    }

    fn attachment_encrypted_signature(&self) -> Option<&AttachmentEncryptedSignature> {
        self.armored_encrypted_signature.as_ref()
    }
}

struct TestAttachmentRaw(&'static str);

impl EncryptableAttachment for TestAttachmentRaw {
    fn attachment_data(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

fn create_test_attachment_metadata() -> TestAttachmentMetdata {
    TestAttachmentMetdata {
        key_packets: KeyPackets::from(TEST_ATTACHMENT_METADATA_KP),
        signature: Some(AttachmentSignature::from(TEST_ATTACHMENT_METADATA_SIG)),
        enc_signature: Some(AttachmentEncryptedSignature::from(
            TEST_ATTACHMENT_METADATA_ENC_SIG,
        )),
    }
}

fn create_test_attachment_metadata_enc_sig_only() -> TestAttachmentMetdata {
    TestAttachmentMetdata {
        key_packets: KeyPackets::from(TEST_ATTACHMENT_METADATA_KP),
        signature: None,
        enc_signature: Some(AttachmentEncryptedSignature::from(
            TEST_ATTACHMENT_METADATA_ENC_SIG,
        )),
    }
}

fn create_test_attachment_encrypted_data() -> Vec<u8> {
    let b64 = base64::engine::general_purpose::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::general_purpose::PAD,
    );
    b64.decode(TEST_ATTACHMENT_ENC_DATA).unwrap()
}

#[test]
fn test_attachment_decrypt() {
    let attachment_metadata = create_test_attachment_metadata();
    test_attachment_decrypt_helper(&attachment_metadata);
}

#[test]
fn test_attachment_decrypt_encrypted_signature() {
    let attachment_metadata = create_test_attachment_metadata_enc_sig_only();
    test_attachment_decrypt_helper(&attachment_metadata);
}

#[test]
fn test_attachment_decrypt_stream() {
    let attachment_metadata = create_test_attachment_metadata();
    test_attachment_decrypt_stream_helper(&attachment_metadata);
}

#[test]
fn test_attachment_decrypt_stream_encrypted_signature() {
    let attachment_metadata = create_test_attachment_metadata_enc_sig_only();
    test_attachment_decrypt_stream_helper(&attachment_metadata);
}

#[test]
fn test_attachment_encrypt_decrypt() {
    test_attachment_encrypt_decrypt_helper(false);
}

#[test]
fn test_attachment_encrypt_decrypt_encrypted_signature() {
    test_attachment_encrypt_decrypt_helper(true);
}

#[test]
fn test_attachment_encrypt_decrypt_stream() {
    test_attachment_encrypt_decrypt_stream_helper(false);
}

#[test]
fn test_attachment_encrypt_decrypt_encrypted_signature_stream() {
    test_attachment_encrypt_decrypt_stream_helper(true);
}

#[test]
fn test_attachment_re_encrypt() {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let address_keys = create_account_unlocked_address_keys(&pgp, TEST_DECRYPTION_KEY, "password");
    let primary_address_key = address_keys.primary_for_mail().expect("No primary key");

    let attachment_raw = TestAttachmentRaw(TEST_ATTACHMENT_PLAIN_DATA);
    let encrypted_attachment = attachment_raw
        .attachment_encrypt_and_sign(&pgp, &primary_address_key)
        .unwrap();

    let attachment_info = DecryptableAttachmentMetadata::new(&pgp, &encrypted_attachment.metadata)
        .decrypt_attachment_info(&pgp, &address_keys)
        .expect("must decrypt");

    let (recipients_priv, recipients_priv_pub) = create_test_recipient_keys(&pgp);

    let key_packets = attachment_info
        .session_key
        .encrypt_to_recipients(&pgp, &recipients_priv_pub)
        .expect("encrypt towards recipient should work");

    let enc_signatures = recipients_priv_pub
        .iter()
        .map(|recipient| {
            attachment_info
                .encrypt_signature_to_recipient(&pgp, recipient)
                .expect("encrypt signature must succeed")
        })
        .collect::<Vec<_>>();

    for ((private_key, key_packet), enc_signature) in recipients_priv
        .iter()
        .zip(key_packets.into_iter())
        .zip(enc_signatures.into_iter())
    {
        let metadata = TestAttachmentMetdata {
            key_packets: KeyPackets::from(key_packet.0),
            signature: None,
            enc_signature: enc_signature.map(|sig| sig.armor(&pgp).unwrap()),
        };
        let dec_result = metadata
            .decrypt(
                &pgp,
                &[private_key],
                &[primary_address_key.for_encryption()],
                &encrypted_attachment.data,
            )
            .expect("should decrypt");
        assert_eq!(dec_result.as_bytes(), TEST_ATTACHMENT_PLAIN_DATA.as_bytes());
        assert!(dec_result.verification_result().is_ok());

        let dec_result_fail = metadata.decrypt(
            &pgp,
            &address_keys,
            &address_keys,
            &encrypted_attachment.data,
        );
        assert!(dec_result_fail.is_err());
    }
}

#[test]
fn test_attachment_re_encrypt_password() {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let address_keys = create_account_unlocked_address_keys(&pgp, TEST_DECRYPTION_KEY, "password");
    let primary_address_key = address_keys.primary_for_mail().expect("No primary key");

    let attachment_raw = TestAttachmentRaw(TEST_ATTACHMENT_PLAIN_DATA);
    let encrypted_attachment = attachment_raw
        .attachment_encrypt_and_sign(&pgp, &primary_address_key)
        .unwrap();

    let attachment_info = DecryptableAttachmentMetadata::new(&pgp, &encrypted_attachment.metadata)
        .decrypt_attachment_info(&pgp, &address_keys)
        .expect("must decrypt");

    let kp = attachment_info
        .encrypt_session_key_to_password(&pgp, "password")
        .unwrap();

    let out = pgp
        .new_decryptor()
        .with_passphrase("password")
        .decrypt_session_key(kp.decode().unwrap())
        .unwrap();
    let b64 = base64::engine::general_purpose::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::general_purpose::PAD,
    );
    let expected = b64.encode(out.export());

    assert_eq!(attachment_info.session_key.expose_secret().0, expected);
}

#[test]
fn test_attachment_encrypt_decrypt_v6() {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let address_keys = create_account_unlocked_address_keys_v6(
        &pgp,
        TEST_DECRYPTION_KEY,
        TEST_DECRYPTION_KEY_V6,
        "password",
    );
    let primary_address_key = address_keys.primary_for_mail().expect("No primary key");

    assert!(primary_address_key.is_v6);
    let attachment_raw = TestAttachmentRaw(TEST_ATTACHMENT_PLAIN_DATA);
    let result = attachment_raw
        .attachment_encrypt_and_sign(&pgp, &primary_address_key)
        .unwrap();

    // Sig should be ok v4
    let decrypted_attachment = DecryptableAttachmentMetadata::new(&pgp, &result.metadata)
        .decrypt(
            &pgp,
            &address_keys,
            &[address_keys.first().unwrap()],
            &result.data,
        )
        .unwrap();

    assert_eq!(
        decrypted_attachment.as_ref(),
        TEST_ATTACHMENT_PLAIN_DATA.as_bytes()
    );

    let verification_result = decrypted_attachment.verification_result();
    assert!(verification_result.is_ok());
}

fn test_attachment_encrypt_decrypt_helper(enc_sig: bool) {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let address_keys = create_account_unlocked_address_keys(&pgp, TEST_DECRYPTION_KEY, "password");
    let primary_address_key = address_keys.primary_for_mail().expect("No primary key");

    let attachment_raw = TestAttachmentRaw(TEST_ATTACHMENT_PLAIN_DATA);
    let mut result = attachment_raw
        .attachment_encrypt_and_sign(&pgp, &primary_address_key)
        .unwrap();

    if enc_sig {
        result.metadata.signature = None;
    }

    // Sig should be ok
    let decrypted_attachment = DecryptableAttachmentMetadata::new(&pgp, &result.metadata)
        .decrypt(&pgp, &address_keys, &address_keys, &result.data)
        .unwrap();

    assert_eq!(
        decrypted_attachment.as_ref(),
        TEST_ATTACHMENT_PLAIN_DATA.as_bytes()
    );

    let verification_result = decrypted_attachment.verification_result();
    assert!(verification_result.is_ok());

    // Sig should be not ok
    let wrong_keys = get_test_public_address_keys(&pgp);
    let decrypted_attachment_wrong = DecryptableAttachmentMetadata::new(&pgp, &result.metadata)
        .decrypt(&pgp, &address_keys, &wrong_keys, &result.data)
        .unwrap();

    let verification_result = decrypted_attachment_wrong.verification_result();
    assert!(verification_result.is_err());
}

fn test_attachment_encrypt_decrypt_stream_helper(enc_sig: bool) {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let address_keys = create_account_unlocked_address_keys(&pgp, TEST_DECRYPTION_KEY, "password");
    let primary_address_key = address_keys.primary_for_mail().expect("No primary key");

    let mut data = Vec::with_capacity(TEST_ATTACHMENT_PLAIN_DATA.len());
    let mut metadata = {
        let mut attachment_writer =
            encrypt_and_sign_to_writer(&pgp, &primary_address_key, &mut data).unwrap();
        attachment_writer
            .write_all(TEST_ATTACHMENT_PLAIN_DATA.as_bytes())
            .unwrap();
        attachment_writer.finalize().unwrap()
    };

    if enc_sig {
        metadata.signature = None;
    }

    // Sig should be ok
    let decrypted_attachment = DecryptableAttachmentMetadata::new(&pgp, &metadata)
        .decrypt(&pgp, &address_keys, &address_keys, &data)
        .unwrap();

    assert_eq!(
        decrypted_attachment.as_ref(),
        TEST_ATTACHMENT_PLAIN_DATA.as_bytes()
    );

    let verification_result = decrypted_attachment.verification_result();
    assert!(verification_result.is_ok());

    // Sig should be not ok
    let wrong_keys = get_test_public_address_keys(&pgp);
    let decrypted_attachment_wrong = DecryptableAttachmentMetadata::new(&pgp, &metadata)
        .decrypt(&pgp, &address_keys, &wrong_keys, &data)
        .unwrap();

    let verification_result = decrypted_attachment_wrong.verification_result();
    assert!(verification_result.is_err());
}

fn test_attachment_decrypt_helper(attachment_metadata: &impl DecryptableAttachment) {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let decryption_keys = get_test_address_keys(&pgp);
    let verification_keys = get_test_public_address_keys(&pgp);

    let enc_data: Vec<u8> = create_test_attachment_encrypted_data();
    let decrypted_attachment = attachment_metadata
        .decrypt(&pgp, &decryption_keys, &verification_keys, enc_data)
        .unwrap();

    assert_eq!(
        decrypted_attachment.as_ref(),
        TEST_ATTACHMENT_PLAIN_DATA.as_bytes()
    );

    let verification_result = decrypted_attachment.verification_result();
    assert!(verification_result.is_ok());
}

fn test_attachment_decrypt_stream_helper(attachment_metadata: &impl DecryptableAttachment) {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let decryption_keys = get_test_address_keys(&pgp);
    let verification_keys = get_test_public_address_keys(&pgp);

    let enc_data: Vec<u8> = create_test_attachment_encrypted_data();
    let mut output_buffer = Vec::new();
    let enc_data_reader: &[u8] = enc_data.as_ref();
    let mut verification_reader = attachment_metadata
        .decrypt_from_reader(&pgp, &decryption_keys, &verification_keys, enc_data_reader)
        .unwrap();
    io::copy(&mut verification_reader, &mut output_buffer).unwrap();
    assert_eq!(&output_buffer, TEST_ATTACHMENT_PLAIN_DATA.as_bytes());

    let verification_result = verification_reader.verification_result();
    assert!(verification_result.is_ok());
}
