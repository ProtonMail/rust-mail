use std::io;

use base64::Engine;
use proton_crypto::crypto::{PrivateKey, PublicKey, VerificationStatus};
use proton_crypto_inbox::attachment::{
    self, AttachmentEncryptedSignature, AttachmentMetadataCryptoView, AttachmentSignature,
    KeyPackets,
};
use proton_crypto_inbox::proton_crypto::crypto::{DataEncoding, PGPProviderSync};

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

const TEST_ATTACHMENT_DECRYPTION_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZWRmVhYJKwYBBAHaRw8BAQdA5Y8bUHq5hTJBWZEa/mxOKJkOOd4h9CVo
2vISFQLcccD+CQMIjfpijTBBdLZgAAAAAAAAAAAAAAAAAAAAADVVVCD463al
FCG7D19/mw35yvsW48YAc3YgmfyK23GC9aptruPrkpjqqxeC6sRve0FxDzA7
Xs0pbHVidXg0QHByb3Rvbi5ibGFjayA8bHVidXg0QHByb3Rvbi5ibGFjaz7C
jAQQFgoAPgWCZWRmVgQLCQcICZDvQqbsF76qjAMVCAoEFgACAQIZAQKbAwIe
ARYhBKcQ8sEYupYe38hwRu9CpuwXvqqMAAB5OQD/XyIK1r+JOFT3cYiBcaFx
iox1yFrsr4uTg8kL1fQPyuoBAIG92J1MoimhMPuYvvTmIvNrvWPZvutw+BF2
hJvRYDYCx4sEZWRmVhIKKwYBBAGXVQEFAQEHQIaaQMB4FXy/xC3qgmlhtnvR
WceanT3nlzFjIrS96RUmAwEIB/4JAwionksZv9YLIGAAAAAAAAAAAAAAAAAA
AAAAGKNKSqywbz5XuXX0Y/zrqKPNIvKBIT/+9dSKlTofYIoP7jtxqdz7UBMb
KkA00FuCKspj/lxrwngEGBYIACoFgmVkZlYJkO9CpuwXvqqMApsMFiEEpxDy
wRi6lh7fyHBG70Km7Be+qowAAHeFAP91gCl/VD/zHEvYIpWEK672jkPUPDpP
Ll+erDsL2C10mgEA5fbBK09OVIjtYUJxiId1YYfn/4/ym92WNEAT20prLww=
-----END PGP PRIVATE KEY BLOCK-----";

const TEST_ATTACHMENT_VERIFICATION_KEY: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----

xjMEZSfovhYJKwYBBAHaRw8BAQdA6gS5mfVImh6ONhKgZGSVrLH4cdZaS9IW
6FhqYGWe2wrNJ2x1YnV4QHByb3Rvbi5ibGFjayA8bHVidXhAcHJvdG9uLmJs
YWNrPsKMBBAWCgA+BYJlJ+i+BAsJBwgJkFX2DKhfS5UBAxUICgQWAAIBAhkB
ApsDAh4BFiEESUD3387U/LDImeGNVfYMqF9LlQEAACDjAPsFKRBgJvErAzLf
7bmk0mK1fwwbFM02LRW86AZE/nTi0QEA72eGf2FJ+5l+9b9Kl1U3xmOaC52P
PFrqPXcklJ7PJAfOOARlJ+i+EgorBgEEAZdVAQUBAQdA87xA21TU/FoSMYoz
1RhyhkNWN7PWcNYut55JEp6S8zcDAQgHwngEGBYIACoFgmUn6L4JkFX2DKhf
S5UBApsMFiEESUD3387U/LDImeGNVfYMqF9LlQEAABKpAQDiqKyJHwQcLpLv
8SxSFLa66KfW1jQpDSPWOauT4cdC6AD+KfaU8KI7/pF1ItedtecaP7uU/rn6
qsxcdv6aoFF1awA=
-----END PGP PUBLIC KEY BLOCK-----";

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
struct TestAddressPubliKey<T: PublicKey>(T);

impl<T: PublicKey> AsRef<T> for TestAddressPubliKey<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl AttachmentMetadataCryptoView for TestAttachmentMetdata {
    fn get_attachment_key_packets(&self) -> &KeyPackets {
        &self.key_packets
    }

    fn get_attachment_signature(&self) -> &Option<AttachmentSignature> {
        &self.signature
    }

    fn get_attachment_encrypted_signature(&self) -> &Option<AttachmentEncryptedSignature> {
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

fn get_test_address_keys<T: PGPProviderSync>(
    pgp_provider: &T,
) -> Vec<TestAddressKey<T::PrivateKey>> {
    let decryption_key = pgp_provider
        .private_key_import(
            TEST_ATTACHMENT_DECRYPTION_KEY,
            "password",
            DataEncoding::Armor,
        )
        .unwrap();
    vec![TestAddressKey(decryption_key)]
}

fn get_test_public_address_keys<T: PGPProviderSync>(
    pgp_provider: &T,
) -> Vec<TestAddressPubliKey<T::PublicKey>> {
    let verification_key = pgp_provider
        .public_key_import(TEST_ATTACHMENT_VERIFICATION_KEY, DataEncoding::Armor)
        .unwrap();
    vec![TestAddressPubliKey(verification_key)]
}

fn get_test_attachment_encrypted_data() -> Vec<u8> {
    let b64 = base64::engine::general_purpose::GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        base64::engine::general_purpose::PAD,
    );
    b64.decode(TEST_ATTACHMENT_ENC_DATA).unwrap()
}

fn test_attachment_decrypt_helper(attachment_metadata: TestAttachmentMetdata) {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let decryption_keys = get_test_address_keys(&pgp_provider);
    let verification_keys = get_test_public_address_keys(&pgp_provider);

    let enc_data: Vec<u8> = get_test_attachment_encrypted_data();
    let decrypted_attachment = attachment::decrypt_attachment(
        &pgp_provider,
        &attachment_metadata,
        &decryption_keys,
        &verification_keys,
        enc_data,
    )
    .unwrap();

    assert_eq!(
        decrypted_attachment.as_ref(),
        TEST_ATTACHMENT_PLAIN_DATA.as_bytes()
    );

    let verification_status = decrypted_attachment.get_verification_status();
    assert!(matches!(verification_status.status, VerificationStatus::Ok));
}

fn test_attachment_decrypt_stream_helper(attachment_metadata: TestAttachmentMetdata) {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();

    let decryption_keys = get_test_address_keys(&pgp_provider);
    let verification_keys = get_test_public_address_keys(&pgp_provider);

    let enc_data: Vec<u8> = get_test_attachment_encrypted_data();
    let mut output_buffer = Vec::new();
    let enc_data_reader: &[u8] = enc_data.as_ref();
    let mut verification_reader = attachment::decrypt_attachment_from_reader(
        &pgp_provider,
        &attachment_metadata,
        &decryption_keys,
        &verification_keys,
        enc_data_reader,
    )
    .unwrap();
    io::copy(&mut verification_reader, &mut output_buffer).unwrap();
    assert_eq!(&output_buffer, TEST_ATTACHMENT_PLAIN_DATA.as_bytes());

    let verification_status = verification_reader.get_verification_status();
    assert!(matches!(verification_status.status, VerificationStatus::Ok));
}

#[test]
fn test_attachment_decrypt() {
    let attachment_metadata = get_test_attachment_metadata();
    test_attachment_decrypt_helper(attachment_metadata);
}

#[test]
fn test_attachment_decrypt_encrypted_signature() {
    let attachment_metadata = get_test_attachment_metadata_enc_sig_only();
    test_attachment_decrypt_helper(attachment_metadata);
}

#[test]
fn test_attachment_decrypt_stream() {
    let attachment_metadata = get_test_attachment_metadata();
    test_attachment_decrypt_stream_helper(attachment_metadata);
}

#[test]
fn test_attachment_decrypt_stream_encrypted_signature() {
    let attachment_metadata = get_test_attachment_metadata_enc_sig_only();
    test_attachment_decrypt_stream_helper(attachment_metadata);
}
