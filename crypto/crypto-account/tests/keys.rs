use proton_crypto::crypto::{DataEncoding, PGPProviderSync};
use proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_account::keys::{
    AddressKeys, DecryptedUserKey, KeyFlag, KeyId, LockedKey, UserKeys,
};
use proton_crypto_account::salts::{Salt, Salts};

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

fn get_test_decrypted_user_key<T: PGPProviderSync>(
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
            private_key:"-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZWRmVhYJKwYBBAHaRw8BAQdA5Y8bUHq5hTJBWZEa/mxOKJkOOd4h9CVo\n2vISFQLcccD+CQMI0hvANzTOSIJggUFyUgQsMpsQzh9uqDb7IbbFWLnI63C1\nm3lKZ4tICeQV4tVFRvHlVRNzJIuTGjFiFbYO1t5ZgcJJgiPEiL5kORqWMOBp\n680pbHVidXg0QHByb3Rvbi5ibGFjayA8bHVidXg0QHByb3Rvbi5ibGFjaz7C\njAQQFgoAPgWCZWRmVgQLCQcICZDvQqbsF76qjAMVCAoEFgACAQIZAQKbAwIe\nARYhBKcQ8sEYupYe38hwRu9CpuwXvqqMAAB5OQD/XyIK1r+JOFT3cYiBcaFx\niox1yFrsr4uTg8kL1fQPyuoBAIG92J1MoimhMPuYvvTmIvNrvWPZvutw+BF2\nhJvRYDYCx4sEZWRmVhIKKwYBBAGXVQEFAQEHQIaaQMB4FXy/xC3qgmlhtnvR\nWceanT3nlzFjIrS96RUmAwEIB/4JAwj8w5GKSR+H62BnDPr48nwPGpA+jvPg\nXG2m4wseURUjdhnVmnLNkC4gJH6wQRz4sqBPye2fHWp+loh+LEDyeBawvkbS\n/FQXNwP7NLSkn84dwngEGBYIACoFgmVkZlYJkO9CpuwXvqqMApsMFiEEpxDy\nwRi6lh7fyHBG70Km7Be+qowAAHeFAP91gCl/VD/zHEvYIpWEK672jkPUPDpP\nLl+erDsL2C10mgEA5fbBK09OVIjtYUJxiId1YYfn/4/ym92WNEAT20prLww=\n=Eckc\n-----END PGP PRIVATE KEY BLOCK-----\n".to_owned(),
            token:Some("-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DcsIsGT18EWcSAQdARTz8SqnWI4HNr+g19xu794pnOQaV0u0GIKbmByr1\n7w8wkWeiYBLW0RmVRP6EPgYLWZoFagItzfCtQYd30RNAKFq33/fjYPDsIXsf\np42uiZ5Q0nEBJb2mMkj8HFEpNw+oeKQUx13OetooxcCald6kVnVQsxx9ZYJ/\np+tmXIoiQmdqSHmqfS6UyAJlyv3T6xqiU7ts5aUTDgS1siMr0UVw6rRLgFp6\npuf9bxNdGMlcmZlvxrMKH+TCodwOQJSXA0IoPDB9Qw==\n=qVb4\n-----END PGP MESSAGE-----\n".to_owned()),
            signature:Some("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwnUEABYKACcFgmV6xP0JkP3x66xOhANrFiEExn1PrCEVWOL10GKE/fHrrE6E\nA2sAACw3AQDJcE5rLsObFILcYBnMMtMIRgk1yJC89wUEmC7HsUUu3wD9FBPO\nasM3eXktszZDtVlk9Yfd+AIxLINr98z/wm1CrgY=\n=2skj\n-----END PGP SIGNATURE-----\n".to_owned()),
            primary: true,
            active: true,
            flags:Some(KeyFlag::from(3_u32)),
            activation: None,
            recovery_secret: None,
            recovery_secret_signature: None,
            address_forwarding_id: None,
        }]
    )
}

fn get_test_locked_user_keys() -> UserKeys {
    let key = LockedKey {
        id: KeyId::from("aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ=="),
        version: 3,
        private_key: "-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdAAp+4PE1Sf5V95XrIY/P2dUNk1TOojoEG\nLuuOzULTa1v+CQMINYn0u3DCV01gjT+Noe2HzLxwP2hieZC1aoGCxSrLn0fs\nLeShqv2pCPZ+SdrjXB5s5Rq7OP5Kr/2gN+0KS0yLGdyirFZWe6m5T8j20UQ5\n0M07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp\nbF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZie3jQQLCQcICZA4nKgbRZBl\nGQMVCAoEFgACAQIZAQKbAwIeARYhBOZJEArPLqrMMxX8fzicqBtFkGUZAADk\n/AD+LA6NW1K+Z3IT66/DEtjH0cmw6HNqxkBdT7kaL2o5pAMA/j9b4JCurWk/\n62MBM4I9RwXzSo8lmgPiYwPp4d/xgEsMx4sEZie3jRIKKwYBBAGXVQEFAQEH\nQHvLC7RWIDsorX5ZmYwjZbUhbXnEcO2sYt8OFaIh5KtHAwEIB/4JAwhKivkG\nshycUGA6wZtPR2HqO6+jvvSlRau/g2eZnWqhnvB4iIYTcD+CPpcPnWrrNgTz\nAU+kQ5sVrP6OiKKHIkUvHT5+MwelTbcpievGx2zGwngEGBYKACoFgmYnt40J\nkDicqBtFkGUZApsMFiEE5kkQCs8uqswzFfx/OJyoG0WQZRkAAJ6BAQDv4nBl\nNnj0W7XiAjiwRmVrY/sdybelB6j01p7UrcVAxQEAtEmT2cSIScVdWH1j3H9l\n0gGE7amH+cm6CjXOA7+Uwwc=\n=RGJ0\n-----END PGP PRIVATE KEY BLOCK-----\n".to_owned(),
        token: None,
        signature: None,
        activation: None,
        primary: true,
        active: true,
        flags: None,
        recovery_secret: None,
        recovery_secret_signature: None,
        address_forwarding_id: None,
    };
    UserKeys(vec![key])
}

fn get_test_salts() -> Salts {
    let salt = Salt {
        id: KeyId::from("aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ=="),
        key_salt: Some("6bIzN4A8bOwmsiEuCPj74g==".to_owned()),
    };
    Salts::new(vec![salt])
}

fn get_test_key_id() -> KeyId {
    KeyId::from(
        "aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ==",
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

#[test]
fn test_user_keys_decrypt() {
    let provider = new_pgp_provider();
    let srp_provider = new_srp_provider();
    let user_keys = get_test_locked_user_keys();
    let key_id = get_test_key_id();
    let salts = get_test_salts();
    // Ok
    let key_secret = salts
        .salt_for_key(&srp_provider, &key_id, "password".as_bytes())
        .unwrap();
    let unlocked_user_key = user_keys.unlock(&provider, &key_secret);
    assert!(unlocked_user_key.unlocked_keys.len() == 1);
    // Faile
    let key_secret = salts
        .salt_for_key(&srp_provider, &key_id, "password1".as_bytes())
        .unwrap();
    let unlocked_user_key = user_keys.unlock(&provider, &key_secret);
    assert!(unlocked_user_key.unlocked_keys.is_empty());
    assert!(unlocked_user_key.failed.len() == 1);
}
