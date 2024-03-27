use proton_crypto_account::proton_crypto::crypto::{AsPublicKeyRef, PrivateKey, PublicKey};
use proton_crypto_inbox::proton_crypto::crypto::{DataEncoding, PGPProviderSync};

pub const TEST_DECRYPTION_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

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

pub const TEST_VERIFICATION_KEY: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----

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

pub struct TestAddressKey<T: PrivateKey>(T);

impl<T: PrivateKey> AsRef<T> for TestAddressKey<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}
pub struct TestAddressPublicKey<T: PublicKey>(T);

impl<T: PublicKey> AsPublicKeyRef<T> for TestAddressPublicKey<T> {
    fn as_public_key(&self) -> &T {
        &self.0
    }
}

pub fn get_test_address_keys<T: PGPProviderSync>(
    pgp_provider: &T,
) -> Vec<TestAddressKey<T::PrivateKey>> {
    get_test_address_key_source(pgp_provider, TEST_DECRYPTION_KEY, "password")
}

pub fn get_test_address_key_source<T: PGPProviderSync>(
    pgp_provider: &T,
    source: &str,
    passphrase: &str,
) -> Vec<TestAddressKey<T::PrivateKey>> {
    let decryption_key = pgp_provider
        .private_key_import(source, passphrase, DataEncoding::Armor)
        .unwrap();
    vec![TestAddressKey(decryption_key)]
}

pub fn get_test_public_address_keys<T: PGPProviderSync>(
    pgp_provider: &T,
) -> Vec<TestAddressPublicKey<T::PublicKey>> {
    get_test_public_address_key_source(pgp_provider, TEST_VERIFICATION_KEY)
}

pub fn get_test_public_address_key_source<T: PGPProviderSync>(
    pgp_provider: &T,
    source: &str,
) -> Vec<TestAddressPublicKey<T::PublicKey>> {
    let verification_key = pgp_provider
        .public_key_import(source, DataEncoding::Armor)
        .unwrap();
    vec![TestAddressPublicKey(verification_key)]
}
