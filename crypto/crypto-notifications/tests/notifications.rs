use proton_crypto_account::proton_crypto::crypto::{
    AsPublicKeyRef, DataEncoding, PGPProviderSync, PrivateKey, PublicKey,
};
use proton_crypto_notifications::{DecryptableNotification, PGPEncryptedNotification};
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

const TEST_NOTIFICATION: &str = "-----BEGIN PGP MESSAGE-----\n\nwV4DI+0bmzSiUhoSAQdAF0IrVwecP1PVnt0OPI4OpwDuqrTnLKEjF5ON0cBqZWUw\noqCfOqAHed/RprRdDhnO9W00MKauHMVA+jCX9Y9J9LFMwzciBVITsZZsJTzuLMBq\n0ksBBGwUT4p8FxbdZEInrA0/2Zn/2F3WIb0Wdz+pbA6RC1LYbzhYOvPV5czMte5h\nzV7CgLG5sY5xN8WwSy+rAu+NcFvfv6ZtlmC1/1M=\n=f9T2\n-----END PGP MESSAGE-----";
const TEST_EXPECTED_NOTIFICATION: DecryptedTestNotification = DecryptedTestNotification {
    data: 1234,
    kind: Kind::Foo,
};

pub struct EncryptedTestNotification(pub String);

impl PGPEncryptedNotification for EncryptedTestNotification {
    fn pgp_encrypted_notification_data(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl DecryptableNotification for EncryptedTestNotification {}

pub struct TestDeviceKey<T: PrivateKey>(T);
impl<T: PrivateKey> AsRef<T> for TestDeviceKey<T> {
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

pub struct TestDevicePublicKey<T: PublicKey>(T);

impl<T: PublicKey> AsPublicKeyRef<T> for TestDevicePublicKey<T> {
    fn as_public_key(&self) -> &T {
        &self.0
    }
}

fn get_test_device_key<T: PGPProviderSync>(pgp_provider: &T) -> TestDeviceKey<T::PrivateKey> {
    get_test_device_key_source(pgp_provider, TEST_DECRYPTION_KEY, "password")
}

#[allow(clippy::missing_panics_doc, dead_code)]
pub fn get_test_device_key_source<T: PGPProviderSync>(
    pgp_provider: &T,
    source: &str,
    passphrase: &str,
) -> TestDeviceKey<T::PrivateKey> {
    let decryption_key = pgp_provider
        .private_key_import(source, passphrase, DataEncoding::Armor)
        .unwrap();
    TestDeviceKey(decryption_key)
}

#[test]
fn decrypt_notification() {
    let pgp_provider = proton_crypto_account::proton_crypto::new_pgp_provider();

    let decryption_key = get_test_device_key(&pgp_provider);

    let test_notification = EncryptedTestNotification(TEST_NOTIFICATION.into());
    let decrypted_notification = test_notification
        .decrypt(&pgp_provider, &decryption_key)
        .unwrap();

    let notification: DecryptedTestNotification = decrypted_notification.inner;
    assert_eq!(notification, TEST_EXPECTED_NOTIFICATION);
}
