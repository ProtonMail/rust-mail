use proton_api_core::auth::UserKeySecret;
use proton_crypto_account::keys::{
    ArmoredPrivateKey, DecryptedUserKey, KeyId, LockedKey, UnlockedUserKey, UnlockedUserKeys,
};
use proton_crypto_account::proton_crypto::crypto::{DataEncoding, PGPProviderSync};
use proton_crypto_account::proton_crypto::new_srp_provider;
use proton_crypto_account::salts::{KeySalt, Salt, Salts};
use std::iter;
// Test data from Proton black user: rust_test@proton.black, pw: TEST_USER_PASSWORD

pub const TEST_USER_MAIL: &str = "rust_test@proton.black";
pub const TEST_USER_ID: &str =
    "ctxnoKsvmlISYpOtESCWNC4tcFbddXmcQ6yyM94YP4tBngrw4O9IKf8jxSLThqZyqFlX972kKwQCPriEeh4qg==";

// Default test user key
const TEST_USER_PASSWORD: &str = "password";
const TEST_USER_KEY_ID: &str =
    "aTdvCsWuv2V_YQQ5nLKsWPkHWMrlHfUxL9aTWakz6blhwI0q_j4MKnxO29xMQ4slCRvo3lFLE8ljb3kvMP2PQQ==";
const TEST_USER_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdAAp+4PE1Sf5V95XrIY/P2dUNk1TOojoEG\nLuuOzULTa1v+CQMINYn0u3DCV01gjT+Noe2HzLxwP2hieZC1aoGCxSrLn0fs\nLeShqv2pCPZ+SdrjXB5s5Rq7OP5Kr/2gN+0KS0yLGdyirFZWe6m5T8j20UQ5\n0M07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp\nbF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZie3jQQLCQcICZA4nKgbRZBl\nGQMVCAoEFgACAQIZAQKbAwIeARYhBOZJEArPLqrMMxX8fzicqBtFkGUZAADk\n/AD+LA6NW1K+Z3IT66/DEtjH0cmw6HNqxkBdT7kaL2o5pAMA/j9b4JCurWk/\n62MBM4I9RwXzSo8lmgPiYwPp4d/xgEsMx4sEZie3jRIKKwYBBAGXVQEFAQEH\nQHvLC7RWIDsorX5ZmYwjZbUhbXnEcO2sYt8OFaIh5KtHAwEIB/4JAwhKivkG\nshycUGA6wZtPR2HqO6+jvvSlRau/g2eZnWqhnvB4iIYTcD+CPpcPnWrrNgTz\nAU+kQ5sVrP6OiKKHIkUvHT5+MwelTbcpievGx2zGwngEGBYKACoFgmYnt40J\nkDicqBtFkGUZApsMFiEE5kkQCs8uqswzFfx/OJyoG0WQZRkAAJ6BAQDv4nBl\nNnj0W7XiAjiwRmVrY/sdybelB6j01p7UrcVAxQEAtEmT2cSIScVdWH1j3H9l\n0gGE7amH+cm6CjXOA7+Uwwc=\n=RGJ0\n-----END PGP PRIVATE KEY BLOCK-----\n";

const TEST_RAW_USER_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZie3jRYJKwYBBAHaRw8BAQdAAp+4PE1Sf5V95XrIY/P2dUNk1TOojoEG
LuuOzULTa1v+CQMILc3WlaItvOVgnwYHR1pyDre1scinyvasQ68h8slWSxxJ
6qeDKX99FK3q+D1oMlw/ZZ6i2RwwP4hB97osleEhddgi/MHs5oOirfkMXm3n
Qs07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp
bF91c2VAZG9tYWluLnRsZD7CjAQQFgoAPgWCZie3jQQLCQcICZA4nKgbRZBl
GQMVCAoEFgACAQIZAQKbAwIeARYhBOZJEArPLqrMMxX8fzicqBtFkGUZAADk
/AD+LA6NW1K+Z3IT66/DEtjH0cmw6HNqxkBdT7kaL2o5pAMA/j9b4JCurWk/
62MBM4I9RwXzSo8lmgPiYwPp4d/xgEsMx4sEZie3jRIKKwYBBAGXVQEFAQEH
QHvLC7RWIDsorX5ZmYwjZbUhbXnEcO2sYt8OFaIh5KtHAwEIB/4JAwi2qY81
wzEON2DmOT/pvwU8EE8Pkg8lFSkRzV0qOwjuRQr5adcQlq3K1+PjoGCmO44t
fwVI9SqKyBkpKWi2Ue5ti4ExSohmMJcQu80IeMCNwngEGBYKACoFgmYnt40J
kDicqBtFkGUZApsMFiEE5kkQCs8uqswzFfx/OJyoG0WQZRkAAJ6BAQDv4nBl
Nnj0W7XiAjiwRmVrY/sdybelB6j01p7UrcVAxQEAtEmT2cSIScVdWH1j3H9l
0gGE7amH+cm6CjXOA7+Uwwc=
=mPy5
-----END PGP PRIVATE KEY BLOCK-----
";

fn testdata_locked_user_key() -> LockedKey {
    LockedKey {
        id: KeyId::from(TEST_USER_KEY_ID),
        version: 3,
        private_key: ArmoredPrivateKey::from(TEST_USER_KEY.to_owned()),
        token: None,
        signature: None,
        activation: None,
        primary: true,
        active: true,
        flags: None,
        recovery_secret: None,
        recovery_secret_signature: None,
        address_forwarding_id: None,
    }
}

/// Returns the user secret to unlock the default test user keys.
pub fn testdata_user_secret() -> UserKeySecret {
    let salts = Salts::new(iter::once(Salt {
        id: KeyId::from(TEST_USER_KEY_ID),
        key_salt: Some(KeySalt::from("6bIzN4A8bOwmsiEuCPj74g==".to_owned())),
    }));
    let locked_key = testdata_locked_user_key();
    let srp_provider = new_srp_provider();
    salts
        .salt_for_key(&srp_provider, &locked_key.id, TEST_USER_PASSWORD.as_bytes())
        .map(UserKeySecret)
        .unwrap()
}

/// Returns the unlocked user keys of the test account.
pub fn unlocked_user_key<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
) -> UnlockedUserKeys<Provider> {
    let private_key = pgp_provider
        .private_key_import(
            TEST_RAW_USER_KEY.as_bytes(),
            "password".as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();
    let public_key = pgp_provider
        .private_key_to_public_key(&private_key)
        .unwrap();
    let user_key: UnlockedUserKey<Provider> = DecryptedUserKey {
        id: KeyId::from(TEST_USER_KEY_ID),
        private_key,
        public_key,
    };
    vec![user_key]
}
