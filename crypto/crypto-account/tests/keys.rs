use proton_crypto::crypto::{DataEncoding, PGPProviderSync};
use proton_crypto::new_pgp_provider;
use proton_crypto_account::domain::{AddressKeys, DecryptedUserKey, KeyFlag, KeyId, LockedKey};

const TEST_USER_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZXrEuBYJKwYBBAHaRw8BAQdAd3SP+S82mvNYec99IYXXy02QlEtWOwCX
G+VRoWMTJgT+CQMIAuL1Bl1uoZBgAAAAAAAAAAAAAAAAAAAAAP8Kb+34nsOQ
jVlCUF4Rco6I2xectxdUsuCm6X+Emq+S+8JsPw/rwVxAmClvKJaeWIfZIV/u
yc07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp
bF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZXrEuAQLCQcICZD98eusToQD
awMVCAoEFgACAQIZAQKbAwIeARYhBMZ9T6whFVji9dBihP3x66xOhANrAADW
1QEA4TDQcWcCskhIbAyLj3eFN9oO4cAv01QnTYuW5p5LvMYA/AyngETI6OGC
+/8UR3hKvmZMnThBMRfbzqg5B96KTIcBx4sEZXrEuBIKKwYBBAGXVQEFAQEH
QCmW61ll1IgTcm8TuNuh92qEGoIzYrRs0fb6ivPBz7YJAwEIB/4JAwh2VqMV
7EJ4WmAAAAAAAAAAAAAAAAAAAAAAjDFyvMguSeKDXNNvviwSK+nf7uqvbUNJ
EEuxjr48kR2A6Cc4OavQJbAAHIVwUG8UQ+PYW/PvwngEGBYKACoFgmV6xLgJ
kP3x66xOhANrApsMFiEExn1PrCEVWOL10GKE/fHrrE6EA2sAAIGYAQCzpA2U
R18gbFL3k6xUaUaRHxZoxBZQ2crLRO1GhgxTxQEAhYFyb7k/0S4XwcDpSgJO
YJWp7nLYBj9YSh4+qOa/5QM=
-----END PGP PRIVATE KEY BLOCK-----
";

const TEST_KEY_PASSWORD: &str = "password";

pub fn get_test_decrypted_user_key<T: PGPProviderSync>(
    provider: &T,
) -> Vec<DecryptedUserKey<T::PrivateKey, T::PublicKey>> {
    let private_key = provider
        .private_key_import(
            TEST_USER_KEY,
            TEST_KEY_PASSWORD.as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();
    let public_key = provider.private_key_to_public_key(&private_key).unwrap();
    vec![DecryptedUserKey {
        id: KeyId::from("G8URRzoYaBW6mSPQjbbo2yYgwI828DVcEs8dDRKxByd1A_qSRYF49TOtw_m4wvDGb76M-r3AVdXuDzSHObR5hQ=="),
        private_key,
        public_key,
    }]
}

pub fn get_test_locked_address_key() -> AddressKeys {
    AddressKeys::new(
        vec![LockedKey {
            id:KeyId::from("ssbW3i5egXM4F-2uqNc2qACsxtKnuYaWMYJsso5IKTLQXLwEDFc_Hib0QaK6QODlGryyLhBH679-UkMkRBSz9w=="),
            version:3,
            private_key:"-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZWRmVhYJKwYBBAHaRw8BAQdA5Y8bUHq5hTJBWZEa/mxOKJkOOd4h9CVo\n2vISFQLcccD+CQMI0hvANzTOSIJggUFyUgQsMpsQzh9uqDb7IbbFWLnI63C1\nm3lKZ4tICeQV4tVFRvHlVRNzJIuTGjFiFbYO1t5ZgcJJgiPEiL5kORqWMOBp\n680pbHVidXg0QHByb3Rvbi5ibGFjayA8bHVidXg0QHByb3Rvbi5ibGFjaz7C\njAQQFgoAPgWCZWRmVgQLCQcICZDvQqbsF76qjAMVCAoEFgACAQIZAQKbAwIe\nARYhBKcQ8sEYupYe38hwRu9CpuwXvqqMAAB5OQD/XyIK1r+JOFT3cYiBcaFx\niox1yFrsr4uTg8kL1fQPyuoBAIG92J1MoimhMPuYvvTmIvNrvWPZvutw+BF2\nhJvRYDYCx4sEZWRmVhIKKwYBBAGXVQEFAQEHQIaaQMB4FXy/xC3qgmlhtnvR\nWceanT3nlzFjIrS96RUmAwEIB/4JAwj8w5GKSR+H62BnDPr48nwPGpA+jvPg\nXG2m4wseURUjdhnVmnLNkC4gJH6wQRz4sqBPye2fHWp+loh+LEDyeBawvkbS\n/FQXNwP7NLSkn84dwngEGBYIACoFgmVkZlYJkO9CpuwXvqqMApsMFiEEpxDy\nwRi6lh7fyHBG70Km7Be+qowAAHeFAP91gCl/VD/zHEvYIpWEK672jkPUPDpP\nLl+erDsL2C10mgEA5fbBK09OVIjtYUJxiId1YYfn/4/ym92WNEAT20prLww=\n=Eckc\n-----END PGP PRIVATE KEY BLOCK-----\n".to_string(),
            token:Some("-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DcsIsGT18EWcSAQdARTz8SqnWI4HNr+g19xu794pnOQaV0u0GIKbmByr1\n7w8wkWeiYBLW0RmVRP6EPgYLWZoFagItzfCtQYd30RNAKFq33/fjYPDsIXsf\np42uiZ5Q0nEBJb2mMkj8HFEpNw+oeKQUx13OetooxcCald6kVnVQsxx9ZYJ/\np+tmXIoiQmdqSHmqfS6UyAJlyv3T6xqiU7ts5aUTDgS1siMr0UVw6rRLgFp6\npuf9bxNdGMlcmZlvxrMKH+TCodwOQJSXA0IoPDB9Qw==\n=qVb4\n-----END PGP MESSAGE-----\n".to_string()),
            signature:Some("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwnUEABYKACcFgmV6xP0JkP3x66xOhANrFiEExn1PrCEVWOL10GKE/fHrrE6E\nA2sAACw3AQDJcE5rLsObFILcYBnMMtMIRgk1yJC89wUEmC7HsUUu3wD9FBPO\nasM3eXktszZDtVlk9Yfd+AIxLINr98z/wm1CrgY=\n=2skj\n-----END PGP SIGNATURE-----\n".to_string()),
            primary:true,
            active:true,
            flags:Some(KeyFlag::from(3_u32)),
            activation: None,
            recovery_secret: None,
            recovery_secret_signature: None,
            address_forwarding_id: None,
        }]
    )
}

#[test]
fn test_address_keys_decrypt() {
    let provider = new_pgp_provider();
    let user_keys = get_test_decrypted_user_key(&provider);
    let address_keys = get_test_locked_address_key();
    let unlocked_keys = address_keys.unlock(&provider, user_keys.as_slice());
    assert!(unlocked_keys.failed.is_empty());
    assert!(!unlocked_keys.unlocked_keys.is_empty());
}
