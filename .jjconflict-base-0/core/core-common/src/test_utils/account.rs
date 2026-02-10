use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::{
    Address as ApiAddress, AddressFlags, AddressId, AddressSignedKeyList,
    AddressStatus as ApiAddressStatus, AddressType as ApiAddressType,
};
use std::sync::LazyLock;

use proton_crypto_account::keys::{
    AddressKeys, ArmoredPrivateKey, DecryptedUserKey, EncryptedKeyToken, KeyFlag, KeyId,
    KeyTokenSignature, LockedKey, UnlockedUserKey, UnlockedUserKeys, UserKeys,
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

// Address keys for TEST_USER_MAIL
pub const TEST_ADDRESS_ID: &str =
    "LGXtB3TbNifsW1elXtCp5zyysma52yRf8NZZ10pUQrJfp1QQCSoFTXcIVDCZJycme6KYHsxCE_xdneJ10dt_iA==";
pub const TEST_ADDRESS_KEY_ID: &str =
    "gzKDANARz0i8OHhGuZV-oFfURju0I3XeW_hNn09g13dS_NJ57UbW420UAcWb-0s93xoav22O_jARq61FyL3guw==";
pub const TEST_ADDRESS_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEZie3jRYJKwYBBAHaRw8BAQdA0lnAs/zJxwALYyLq9jnthTTJauaqwvLQ\nod3cCVOua+v+CQMIcWjkpeADcjxgwP+7tEc2sfM3J4oWV/p344AsSBiK442t\n5GmxcPBNuj7P82Mjfj10MfhzxIgDF39KW85vcrL4BRuDYq4uSUURFnZmiLFS\nx80vcnVzdF90ZXN0QHByb3Rvbi5ibGFjayA8cnVzdF90ZXN0QHByb3Rvbi5i\nbGFjaz7CjAQQFgoAPgWCZie3jQQLCQcICZDD5SnHczmG6wMVCAoEFgACAQIZ\nAQKbAwIeARYhBBGxOGij+OleubdsX8PlKcdzOYbrAABxyQEA53ij2BO8KHOi\nlmhaB9qeaNDnZhlvNazM9O87r2Cm03UA/jLgvtPQe+HgIDbguMFSeacvAKSG\n2A5jl6AAPWjifF4Jx4sEZie3jRIKKwYBBAGXVQEFAQEHQLJ401cWczKQigvx\njfQ5DxVXvA9p+HRuW16642Ybd99+AwEIB/4JAwjsnBN5czXnymCSAHHIugJH\nwwH1rvooZGeZ26QZ/UhsjQwXy1O5J66plmBD1Oe/uZG4Ed6ylw1VwROmW03q\nrRWwYeeVSN20YMavgbAZT7AVwngEGBYKACoFgmYnt40JkMPlKcdzOYbrApsM\nFiEEEbE4aKP46V65t2xfw+Upx3M5husAAPU7AQCMKF564vtdGCY/KIGqAhm2\nSNUnK5w6MkGKgrztbAhvngD/VK3t0WB8mUqXC3JoS2xC6rtyiyciAjQvuwWT\n2ePDxgI=\n=5IIS\n-----END PGP PRIVATE KEY BLOCK-----\n";
pub const TEST_ADDRESS_KEY_TOKEN: &str = "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DJ8rw1vR308gSAQdAwfey4aUSny0pDcCM0OykFF+KoquoUEuc5I48NYNn\nNkYwdMVXcHgrNAOVkSgBcCS5VxaRb3Lmo610XkQRnCyuadgvce4pRFqtx0+A\nNCNgn/Px0nEB+tPsQJL+EePQHgMZXhXmW3tS6/7jxzyCkuJVKdXHFNu3kTNU\nthAEwWkLUrQu280+De/2UEFq8oB6vjvUJiohremKSNp2Wr8fhL+XQubLoCtw\nln9Pw5EL3607i64Cs5f88Ew35GeKPQw/uUuCI8uB0A==\n=dj6J\n-----END PGP MESSAGE-----\n";
pub const TEST_ADDRESS_KEY_SIGNATURE: &str = "-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwnUEARYKACcFgmYnt8kJkDicqBtFkGUZFiEE5kkQCs8uqswzFfx/OJyoG0WQ\nZRkAACZ4AP49xBDsaIUR1IEJlMqTdwaSJ+02eXXpJANwT/mg2QNTJwD/fXhq\nojjc2LEMrebiFAl4GQgXxkUgnPuvpCyiB80C3A8=\n=KsBO\n-----END PGP SIGNATURE-----\n";

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

fn testdata_locked_address_key() -> LockedKey {
    LockedKey {
        id: KeyId::from(TEST_ADDRESS_KEY_ID),
        version: 3,
        private_key: ArmoredPrivateKey::from(TEST_ADDRESS_KEY.to_owned()),
        token: Some(EncryptedKeyToken::from(TEST_ADDRESS_KEY_TOKEN.to_owned())),
        signature: Some(KeyTokenSignature::from(
            TEST_ADDRESS_KEY_SIGNATURE.to_owned(),
        )),
        activation: None,
        primary: true,
        active: true,
        flags: Some(KeyFlag::from(3_u8)),
        recovery_secret: None,
        recovery_secret_signature: None,
        address_forwarding_id: None,
    }
}

/// Returns the default test user keys.
#[must_use]
pub fn testdata_user_keys() -> UserKeys {
    UserKeys(vec![testdata_locked_user_key()])
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

/// Returns the address keys for the default test user matching its address [`TEST_USER_MAIL`].
#[must_use]
pub fn testdata_address_keys_for_user_address() -> AddressKeys {
    AddressKeys(vec![testdata_locked_address_key()])
}

/// Returns the unlocked user keys of the test account.
///
pub fn unlocked_user_key<P>(pgp: &P) -> UnlockedUserKeys<P>
where
    P: PGPProviderSync,
{
    let private_key = pgp
        .private_key_import(
            TEST_RAW_USER_KEY.as_bytes(),
            "password".as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();

    let public_key = pgp.private_key_to_public_key(&private_key).unwrap();

    let user_key: UnlockedUserKey<P> = DecryptedUserKey {
        id: KeyId::from(TEST_USER_KEY_ID),
        private_key,
        public_key,
    };

    UnlockedUserKeys::from(vec![user_key])
}

pub const TEST_ADDRESS_EMAIL: &str = "hello@world";
pub static MY_ADDRESS_ID: LazyLock<AddressId> = LazyLock::new(|| AddressId::from("MyRemoteId"));

#[must_use]
pub fn test_api_address() -> ApiAddress {
    ApiAddress {
        id: MY_ADDRESS_ID.clone(),
        email: TEST_ADDRESS_EMAIL.to_owned(),
        send: true,
        receive: true,
        status: ApiAddressStatus::Enabled,
        domain_id: None,
        address_type: ApiAddressType::Original,
        order: 0,
        display_name: "HelloWorld".to_owned(),
        signature: "SIGNATURE".to_owned(),
        keys: AddressKeys::new(vec![]),
        catch_all: false,
        proton_mx: false,
        signed_key_list: AddressSignedKeyList::default(),
        flags: AddressFlags::default(),
    }
}
