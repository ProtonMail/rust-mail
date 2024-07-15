use proton_crypto_account::{
    keys::{DecryptedAddressKey, KeyFlag, KeyId, UnlockedAddressKey},
    proton_crypto::crypto::{AsPublicKeyRef, PrivateKey, PublicKey},
};
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

#[allow(clippy::missing_panics_doc, dead_code)]
pub fn get_test_address_keys<T: PGPProviderSync>(
    pgp_provider: &T,
) -> Vec<TestAddressKey<T::PrivateKey>> {
    get_test_address_key_source(pgp_provider, TEST_DECRYPTION_KEY, "password")
}

#[allow(clippy::missing_panics_doc, dead_code)]
pub fn create_account_unlocked_address_key<T: PGPProviderSync>(
    provider: &T,
    source: &str,
    passphrase: &str,
) -> UnlockedAddressKey<T> {
    let private_key = provider
        .private_key_import(source, passphrase, DataEncoding::Armor)
        .unwrap();
    let public_key = provider.private_key_to_public_key(&private_key).unwrap();
    DecryptedAddressKey {
        id: KeyId::from("address key"),
        private_key,
        public_key,
        flags: KeyFlag::default(),
        primary: true,
    }
}

#[allow(clippy::missing_panics_doc, dead_code)]
pub fn create_test_recipient_keys<Provider: PGPProviderSync>(
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

#[allow(clippy::missing_panics_doc, dead_code)]
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

#[allow(clippy::missing_panics_doc, dead_code)]
pub fn get_test_public_address_keys<T: PGPProviderSync>(
    pgp_provider: &T,
) -> Vec<TestAddressPublicKey<T::PublicKey>> {
    get_test_public_address_key_source(pgp_provider, TEST_VERIFICATION_KEY)
}

#[allow(clippy::missing_panics_doc)]
pub fn get_test_public_address_key_source<T: PGPProviderSync>(
    pgp_provider: &T,
    source: &str,
) -> Vec<TestAddressPublicKey<T::PublicKey>> {
    let verification_key = pgp_provider
        .public_key_import(source, DataEncoding::Armor)
        .unwrap();
    vec![TestAddressPublicKey(verification_key)]
}
