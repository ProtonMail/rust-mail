use proton_crypto_account::proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, Encryptor, EncryptorSync, KeyGenerator, KeyGeneratorSync,
    PGPMessage, PGPProviderSync, PrivateKey, PublicKey, Signer, SignerSync,
};
use proton_crypto_notifications::{DecryptableNotification, GettablePGPNotification};
use serde::{Deserialize, Serialize};

pub const TEST_DECRYPTION_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZ7h2qBYJKwYBBAHaRw8BAQdACCs1pYsssVQCyYL3EvbuKMKxtly8bAwEHcUA
HFG56Av+CQMI59Gslp2yCp5gkTHem6UuDFtmeKiJNqUHWjVnZqEQdinG/pVddve8
fBCfIQNeCIag6y5w3i74acVa4DdZU18MxoM0UaQVmskA3vul+e35H80VdGVzdCA8
dGVzdEB0ZXN0LnRlc3Q+wsAQBBMWCgCCBYJnuHaoAwsJBwmQXSdp3d8v+9dFFAAA
AAAAHAAgc2FsdEBub3RhdGlvbnMub3BlbnBncGpzLm9yZ6UDaRR9NcNS/9Y9Kq/M
eSFHp4qW4u8rki7KQBhfkxkdAxUKCAMWAAICGQECmwMCHgEWIQTdQPSmcOCEJpko
XrRdJ2nd3y/71wAAbdYBAPHR11prhn0pMasXbHqdDnTr17Y8cvjW9nE1A2kRNbvj
AP9Ndg/4PST9Yzte6rIsDEfQzbYi2Q+QwMYn2DtgVTPQCMeLBGe4dqgSCisGAQQB
l1UBBQEBB0BeSwqSiFjbJ0+4Gs+bXO2SCZiiA22zkVbvQTvsglPEPQMBCgn+CQMI
59Gslp2yCp5gzwS1jXQI7rY1iJzmHECo7oAuxXe34t/+10K3hPwo5GR+R98mvkxm
irEkkvRcTFTfQUAzhUp7yAaFwwf35BPDjfW8ZqTRSMK+BBgWCgBwBYJnuHaoCZBd
J2nd3y/710UUAAAAAAAcACBzYWx0QG5vdGF0aW9ucy5vcGVucGdwanMub3JnnHYd
NCdrCTgZtaBaltIPSfnRPpJJ0i6NTOsFsIdoWtwCmwwWIQTdQPSmcOCEJpkoXrRd
J2nd3y/71wAAee0A/jpeJyC8vLfUiInmP5hNlYC/zQV3rDClj7oIvyTyU6DqAQCR
cNYbWd7Rzfdgv/WxEO8Ko3qDwvmEgeTnRmUAOzapDA==
=P28z
-----END PGP PRIVATE KEY BLOCK-----";
pub const TEST_VERIFICATION_KEY: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----

xjMEZ7h2qBYJKwYBBAHaRw8BAQdACCs1pYsssVQCyYL3EvbuKMKxtly8bAwEHcUA
HFG56AvNFXRlc3QgPHRlc3RAdGVzdC50ZXN0PsLAEAQTFgoAggWCZ7h2qAMLCQcJ
kF0nad3fL/vXRRQAAAAAABwAIHNhbHRAbm90YXRpb25zLm9wZW5wZ3Bqcy5vcmel
A2kUfTXDUv/WPSqvzHkhR6eKluLvK5IuykAYX5MZHQMVCggDFgACAhkBApsDAh4B
FiEE3UD0pnDghCaZKF60XSdp3d8v+9cAAG3WAQDx0ddaa4Z9KTGrF2x6nQ5069e2
PHL41vZxNQNpETW74wD/TXYP+D0k/WM7XuqyLAxH0M22ItkPkMDGJ9g7YFUz0AjO
OARnuHaoEgorBgEEAZdVAQUBAQdAXksKkohY2ydPuBrPm1ztkgmYogNts5FW70E7
7IJTxD0DAQoJwr4EGBYKAHAFgme4dqgJkF0nad3fL/vXRRQAAAAAABwAIHNhbHRA
bm90YXRpb25zLm9wZW5wZ3Bqcy5vcmecdh00J2sJOBm1oFqW0g9J+dE+kknSLo1M
6wWwh2ha3AKbDBYhBN1A9KZw4IQmmShetF0nad3fL/vXAAB57QD+Ol4nILy8t9SI
ieY/mE2VgL/NBXesMKWPugi/JPJToOoBAJFw1htZ3tHN92C/9bEQ7wqjeoPC+YSB
5OdGZQA7NqkM
=zZPq
-----END PGP PUBLIC KEY BLOCK-----";

const TEST_NOTIFICATION: &str = "-----BEGIN PGP MESSAGE-----\n\nwV4DI+0bmzSiUhoSAQdAF0IrVwecP1PVnt0OPI4OpwDuqrTnLKEjF5ON0cBqZWUw\noqCfOqAHed/RprRdDhnO9W00MKauHMVA+jCX9Y9J9LFMwzciBVITsZZsJTzuLMBq\n0ksBBGwUT4p8FxbdZEInrA0/2Zn/2F3WIb0Wdz+pbA6RC1LYbzhYOvPV5czMte5h\nzV7CgLG5sY5xN8WwSy+rAu+NcFvfv6ZtlmC1/1M=\n=f9T2\n-----END PGP MESSAGE-----";
const TEST_EXPECTED_NOTIFICATION: DecryptedTestNotification = DecryptedTestNotification {
    data: 1234,
    kind: Kind::Foo,
};

pub struct EncryptedTestNotification(pub String);

impl GettablePGPNotification for EncryptedTestNotification {
    fn pgp_notification(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl DecryptableNotification for EncryptedTestNotification {}

pub struct TestUserKey<T: PrivateKey>(T);
impl<T: PrivateKey> AsRef<T> for TestUserKey<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

#[derive(PartialEq, Debug, Deserialize, Serialize)]
struct DecryptedTestNotification {
    pub data: usize,
    #[serde(rename = "type")]
    pub kind: Kind,
}

#[derive(PartialEq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Foo,
}

pub struct TestUserPublicKey<T: PublicKey>(T);

impl<T: PublicKey> AsPublicKeyRef<T> for TestUserPublicKey<T> {
    fn as_public_key(&self) -> &T {
        &self.0
    }
}

fn get_test_user_keys<T: PGPProviderSync>(pgp_provider: &T) -> Vec<TestUserKey<T::PrivateKey>> {
    get_test_user_key_source(pgp_provider, TEST_DECRYPTION_KEY, "password")
}

#[allow(clippy::missing_panics_doc, dead_code)]
pub fn get_test_public_user_keys<T: PGPProviderSync>(
    pgp_provider: &T,
) -> Vec<TestUserPublicKey<T::PublicKey>> {
    get_test_public_user_key_source(pgp_provider, TEST_VERIFICATION_KEY)
}

#[allow(clippy::missing_panics_doc)]
pub fn get_test_public_user_key_source<T: PGPProviderSync>(
    pgp_provider: &T,
    source: &str,
) -> Vec<TestUserPublicKey<T::PublicKey>> {
    let verification_key = pgp_provider
        .public_key_import(source, DataEncoding::Armor)
        .unwrap();
    vec![TestUserPublicKey(verification_key)]
}

#[allow(clippy::missing_panics_doc, dead_code)]
pub fn get_test_user_key_source<T: PGPProviderSync>(
    pgp_provider: &T,
    source: &str,
    passphrase: &str,
) -> Vec<TestUserKey<T::PrivateKey>> {
    let decryption_key = pgp_provider
        .private_key_import(source, passphrase, DataEncoding::Armor)
        .unwrap();
    vec![TestUserKey(decryption_key)]
}

#[test]
fn decrypt_notification() {
    let pgp_provider = proton_crypto_account::proton_crypto::new_pgp_provider();

    let decryption_keys = get_test_user_keys(&pgp_provider);
    // let verification_keys = get_test_public_user_keys(&pgp_provider);

    let test_notification = EncryptedTestNotification(TEST_NOTIFICATION.into());
    let (decrypted_notification, _) = test_notification
        .decrypt(&pgp_provider, &decryption_keys)
        .unwrap();

    let notification: DecryptedTestNotification = decrypted_notification.inner;
    assert_eq!(notification, TEST_EXPECTED_NOTIFICATION);
}

// #[test]
// fn gen_key() {
//     let pgp_provider = proton_crypto_account::proton_crypto::new_pgp_provider();

//     let private_key = pgp_provider
//         .new_key_generator()
//         .with_user_id("test", "test@test.test")
//         .generate()
//         .unwrap();

//     let public_key = pgp_provider
//         .private_key_to_public_key(&private_key)
//         .unwrap();

//     let private_key_e = pgp_provider
//         .private_key_export(&private_key, "password", DataEncoding::Armor)
//         .unwrap();
//     let private_key_e = String::from_utf8_lossy(private_key_e.as_ref());

//     let public_key_e = pgp_provider
//         .public_key_export(&public_key, DataEncoding::Armor)
//         .unwrap();
//     let public_key_e = String::from_utf8_lossy(public_key_e.as_ref());

//     println!("Public: `{}`", public_key_e);
//     println!("Private: `{}`", private_key_e);
//     todo!()
// }

// #[test]
// fn encrypt_notification() {
//     let pgp_provider = proton_crypto_account::proton_crypto::new_pgp_provider();

//     let encryption_keys = get_test_public_user_keys(&pgp_provider);

//     let s = serde_json::to_string(&TEST_EXPECTED_NOTIFICATION).unwrap();

//     let encrypted = pgp_provider
//         .new_encryptor()
//         .with_encryption_key_refs(encryption_keys.as_slice())
//         .encrypt(s.as_bytes())
//         .unwrap();

//     // pgp_provider
//     //     .new_signer()
//     //     .with_signing_key_refs(&encryption_keys)
//     // .sign_inline(encrypted., out_encoding)

//     let data = encrypted.armor().unwrap();
//     let s = String::from_utf8(data).unwrap();

//     dbg!(&s);
//     todo!()
// }
