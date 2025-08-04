use proton_crypto::crypto::{KeyGeneratorAlgorithm, PGPProviderSync};
use proton_crypto::{new_pgp_provider, new_srp_provider};
use proton_crypto_account::keys::{
    KeyFlag, KeyId, LocalAddressKey, LocalUserKey, UnlockedAddressKey, UnlockedAddressKeys,
    UnlockedUserKey, UnlockedUserKeys,
};
use proton_crypto_account::salts::KeySalt;
use proton_crypto_calendar::{
    CalendarEventDecryptor, CalendarEventEncryptor, UnlockedCalendarKey, UnlockedKeys,
};

#[test]
#[allow(clippy::similar_names)]
fn export_and_import_key() {
    let pgp1 = new_pgp_provider();
    let pgp2 = new_pgp_provider();

    let (user_key1, address_key1) = key(&pgp1);
    let (user_key2, address_key2) = key(&pgp1);
    let (user_key3, address_key3) = key(&pgp1);

    let user_keys = UnlockedUserKeys::from(vec![user_key1, user_key2, user_key3]);
    let address_keys = UnlockedAddressKeys(vec![address_key1, address_key2, address_key3]);
    let calendar_key = UnlockedCalendarKey::generate(&pgp1).unwrap();

    let keys = UnlockedKeys {
        user_keys,
        address_keys,
    };

    // ---
    // Case 1: Using user key

    for key in keys.user_keys.iter() {
        calendar_key
            .export(&pgp1, key)
            .unwrap()
            .import(&pgp2, &keys)
            .unwrap();
    }

    // ---
    // Case 2: Using address key

    for key in keys.address_keys.iter() {
        calendar_key
            .export(&pgp1, key)
            .unwrap()
            .import(&pgp2, &keys)
            .unwrap();
    }
}

#[test]
fn encrypt_and_decrypt_events() {
    let pgp = new_pgp_provider();
    let address_keys = UnlockedAddressKeys(vec![key(&pgp).1, key(&pgp).1, key(&pgp).1]);
    let calendar_key = UnlockedCalendarKey::generate(&pgp).unwrap();

    // ---
    // Case 1: Using address key

    let actual = {
        let encryptor = CalendarEventEncryptor::for_address(&pgp, &address_keys).unwrap();
        let (event, sig) = encryptor.encrypt(&pgp, b"Hello, World!").unwrap();
        let key_packets = encryptor.finish(&pgp).unwrap();

        CalendarEventDecryptor::new(&pgp, &address_keys, &calendar_key, key_packets.as_ref())
            .unwrap()
            .decrypt(&pgp, event.as_ref(), Some(sig.as_ref()))
            .unwrap()
            .into_bytes()
    };

    assert_eq!(b"Hello, World!", actual.as_slice());

    // ---
    // Case 2: Using calendar key

    let actual = {
        let encryptor =
            CalendarEventEncryptor::for_calendar(&pgp, &address_keys, &calendar_key).unwrap();

        let (event, sig) = encryptor.encrypt(&pgp, b"Hello, World!").unwrap();
        let key_packets = encryptor.finish(&pgp).unwrap();

        CalendarEventDecryptor::new(&pgp, &address_keys, &calendar_key, key_packets.as_ref())
            .unwrap()
            .decrypt(&pgp, event.as_ref(), Some(sig.as_ref()))
            .unwrap()
            .into_bytes()
    };

    assert_eq!(b"Hello, World!", actual.as_slice());
}

fn key<P>(pgp: &P) -> (UnlockedUserKey<P>, UnlockedAddressKey<P>)
where
    P: PGPProviderSync,
{
    let srp = new_srp_provider();
    let salt = KeySalt::generate();

    let key_secret = salt
        .salted_key_passphrase(&srp, "password".as_bytes())
        .unwrap();

    let user_key = LocalUserKey::generate(pgp, KeyGeneratorAlgorithm::default(), &key_secret)
        .unwrap()
        .unlock_and_assign_key_id(pgp, KeyId(String::default()), &key_secret)
        .unwrap();

    let address_key = LocalAddressKey::generate(
        pgp,
        "someone@localhost",
        KeyGeneratorAlgorithm::default(),
        KeyFlag::default(),
        true,
        &user_key,
    )
    .unwrap()
    .unlock_and_assign_key_id(pgp, KeyId(String::new()), &user_key)
    .unwrap();

    (user_key, address_key)
}
