use common::TEST_VERIFICATION_KEY;
use proton_crypto_account::keys::{
    APIPublicAddressKeyGroup, APIPublicAddressKeys, APIPublicKey, APIPublicKeySource,
    APIUnverifiedPublicAddressKeyGroup, DecryptedAddressKey, EmailMimeType, KeyFlag, KeyId,
    PGPScheme, PinnedPublicKeys, PublicAddressKeys, SKLSignature, SignedKeyList,
};
use proton_crypto_inbox::{
    keys::{
        ComposerPreference, CryptoMailSettings, EncryptionPreferencesError,
        InboxVerificationPreferences, PackageCryptoType, SendPreferences,
    },
    message::packages::PackageMimeType,
    proton_crypto::{
        crypto::{AccessKeyInfo, DataEncoding, PGPProviderSync, UnixTimestamp},
        new_pgp_provider,
    },
};

mod common;

const TEST_KEY: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----
Version: ProtonMail

xjMEZW86jxYJKwYBBAHaRw8BAQdAQOc3jVxw1ISyaSKde3UJ7ZH5foMrjeCV
NWNm8uHmqOnNKWx1YnV4MkBwcm90b24uYmxhY2sgPGx1YnV4MkBwcm90b24u
YmxhY2s+wowEEBYKAD4FgmVvOo8ECwkHCAmQGQDOlJmIZYgDFQgKBBYAAgEC
GQECmwMCHgEWIQTSZZlb0pFeKO6tpwcZAM6UmYhliAAADqcBAMBEiBTMSpoW
0RiXd8wOVl37EyGd39rx0IlGsjsI77AQAP9VsjMZLAD6HU2SYwiL5EF2wHpP
OUcDZqVMpnL9aaJeBsKoBBAWCABaBQJlbzrpCRDU0hoFUaey7BYhBDYVQ78N
pW2mDLPalNTSGgVRp7LsLBxUZXN0IE9wZW5QR1AgQ0EgPHRlc3Qtb3BlbnBn
cC1jYUBwcm90b24ubWU+BYMA7U4AAABVpgEApRWyrfiiJKsSl+Y/kWApsHgN
AgSLLTsXXFxjpUg88ggA/iAIkVfZBOvLlDMdcuGPXliZythV5A292gekdlH+
0SoIzjgEZW86jxIKKwYBBAGXVQEFAQEHQLVv/2vApjXs2rWnbzfkqDWiBA5X
j46YndFrAia0Fa10AwEIB8J4BBgWCgAqBYJlbzqPCZAZAM6UmYhliAKbDBYh
BNJlmVvSkV4o7q2nBxkAzpSZiGWIAAAFkgEApdB1yTmSFV+QcrgsGSZ7veyF
TupI/rjj+Y8rceHcBkcBALaLyrpX7cUeY0yX2MZhPmpiJeE4+4Rot8PIGkVa
X08A
=fBDT
-----END PGP PUBLIC KEY BLOCK-----
";

const PRIVATE_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xX0GY4d/4xsAAAAg+U2nu0jWCmHlZ3BqZYfQMxmZu52JGggkLq2EVD34laP+HQcL
Awgr/Ssmlogji+ACZVkAJhSw8ixv8qOdigzBa/6C38y9kNF+6z8p0p7QogkBoptJ
eKSRqtw0fpcZZwpOEsKMV8PvmPFD0U8VMG9kvGMU7cKxBh8bCgAAAEIFgmOHf+MD
CwkHBRUKDggMAhYAApsDAh4JIiEGyxhsTwYJppfk1S36bHIrDB8eJ8GKVnCPZSXs
J7rZrMkFJwkCBwIAAAAArSggED4tfSJ+wObXzkRx2za/yXCDJTaQJxSYp+8FdsB/
quFFhbO5A7ASfsT9ovAjBFoux2vLT5VxqWUeFK7hE3odZoRCyI+VHjPE/9M/uaF9
UR7tdY/G2cxQy1/Xk7IDnVgEx30GY4d/4xkAAAAghpMkg2f55QFduSL49ICV3aeE
mH8tWYWxL7rRbK9eRDX+HQcLAwgr/Ssmlogji+ByP40pWjHluaiB3cUHpIU3h69K
TXWNUyIsltFCLkpnGCJk3tj8D267qpVCcJS5Q8s0dd5tyyENmsfpodQTyMzGKM2U
N8KbBhgbCgAAACwFgmOHf+MCmwwiIQbLGGxPBgmml+TVLfpscisMHx4nwYpWcI9l
JewnutmsyQAAAAAEASCm6RhtnVk1/I/lYxTNtSdIalpRIPm3YqI1pynwOQEKVlFr
ZzcAxDNINdr2MaFjPGPNVvmxwcPNOSPJFlZF1OrxTovh1r7/4q2u6HybtejZ6FJI
XJZFK5NJl7m2b8peBgY=
-----END PGP PRIVATE KEY BLOCK-----";

const EXPIRED_KEY: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----

xjMEZNm9VhYJKwYBBAHaRw8BAQdA8sshxTpbTURAcXWBHNM1GWVBEb8kGHhySs+A
5lzbiu/NEHRlc3QgPHRlc3RAdGVzdD7CwCAEExYKAJIFgmTZvVYFiQCD1gADCwkH
CZCAMMN38oNOb0UUAAAAAAAcACBzYWx0QG5vdGF0aW9ucy5vcGVucGdwanMub3Jn
yjxvKxHfyibz5kGfP3kImYw7ZpLISloYJ+0cV7ohZZADFQoIAxYAAgIZAQKbAwIe
CRYhBMlSRBcHnzCOxLtoX4Aww3fyg05vCScJAwkCBwMHAgAA2ygA/14FSnN1Bke0
sDrTaqSQ873jpNiAUs1lWniPuySXwu2KAP4xLxyqUSowi3YA6ed81n629KfoD4/K
w6m7jBuJEYmJAs44BGTZvVYSCisGAQQBl1UBBQEBB0A6XcocTYgjWJrn8Hm+NlV4
XCXxe+CuHl2wMvRoJQENGAMBCgnCvgQYFgoAcAWCZNm9VgmQgDDDd/KDTm9FFAAA
AAAAHAAgc2FsdEBub3RhdGlvbnMub3BlbnBncGpzLm9yZ0MrnnNo7qhgW0p6QhNs
mTifGIN2g/gY9CxhM7G+7C3lApsMFiEEyVJEFwefMI7Eu2hfgDDDd/KDTm8AAL5V
AQC3F++meLQ6GJ98gt4q9OSySJ4P8FJV1nyxjT8sanfbMgD7BnzNoFla+QKsI53D
KO/aov7gyQSWLU84geCOwJsXpAs=
=CqK+
-----END PGP PUBLIC KEY BLOCK-----";

const PRIVATE_KEY_PASSWORD: &str = "password";

#[test]
fn test_verification_preferences() {
    let pgp = new_pgp_provider();
    let pinned_keys = create_test_pinned_key(&pgp, TEST_KEY);
    let api_keys = create_test_public_key(&pgp, true);
    let verification_preferences =
        InboxVerificationPreferences::from_public_keys(api_keys, Some(pinned_keys));
    assert!(verification_preferences.compromised_fingerprints.is_empty());
    assert!(verification_preferences.uses_pinned_keys());
    assert_eq!(verification_preferences.pinned_keys.len(), 1);
    assert_eq!(verification_preferences.api_keys.len(), 1);
}

#[test]
fn test_verification_preferences_compromised() {
    let pgp = new_pgp_provider();
    let mut api_keys = create_test_public_key(&pgp, true);
    let pinned_keys = create_test_pinned_key(&pgp, TEST_KEY);
    api_keys
        .address
        .keys
        .first_mut()
        .unwrap()
        .flags
        .set_compromised();
    let verification_preferences =
        InboxVerificationPreferences::from_public_keys(api_keys, Some(pinned_keys));
    assert!(verification_preferences.pinned_keys.is_empty());
    assert!(verification_preferences.api_keys.is_empty());
    assert_eq!(verification_preferences.compromised_fingerprints.len(), 1);
}

#[test]
fn test_verification_preferences_own() {
    let pgp = new_pgp_provider();
    let address_keys = create_test_decrypted_address_key(&pgp);
    let verification_preferences =
        InboxVerificationPreferences::from_unlocked_address_keys(&address_keys);
    assert!(verification_preferences.pinned_keys.is_empty());
    assert!(verification_preferences.compromised_fingerprints.is_empty());
    assert_eq!(verification_preferences.api_keys.len(), 1);
    assert!(!verification_preferences.uses_pinned_keys());
}

#[test]
fn test_verification_preferences_own_compromised() {
    let pgp = new_pgp_provider();
    let mut address_keys = create_test_decrypted_address_key(&pgp);
    address_keys.first_mut().unwrap().flags.set_compromised();
    let verification_preferences =
        InboxVerificationPreferences::from_unlocked_address_keys(&address_keys);
    assert!(verification_preferences.pinned_keys.is_empty());
    assert!(verification_preferences.api_keys.is_empty());
    assert_eq!(verification_preferences.compromised_fingerprints.len(), 1);
}

#[test]
fn test_sending_preferences() {
    let pgp = new_pgp_provider();
    let expected_key = pgp
        .public_key_import(TEST_KEY, DataEncoding::Armor)
        .unwrap();
    let pinned_keys = create_test_pinned_key(&pgp, TEST_KEY);
    let api_keys = create_test_public_key(&pgp, true);
    let mail_setting = CryptoMailSettings {
        pgp_scheme: PGPScheme::PGPMime,
        sign: true,
    };

    let composer_preference = ComposerPreference::new(EmailMimeType::Html);

    let sending_preferences = SendPreferences::new(
        api_keys.clone(),
        Some(pinned_keys.clone()),
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    )
    .expect("should be able to extract sending preferences");

    assert!(
        sending_preferences.encrypt
            && sending_preferences.sign
            && sending_preferences.is_selected_key_pinned
            && !sending_preferences.encryption_disabled
    );
    assert_eq!(
        sending_preferences.selected_key.unwrap().key_fingerprint(),
        expected_key.key_fingerprint()
    );
    assert_eq!(
        sending_preferences.pgp_scheme,
        PackageCryptoType::ProtonMail
    );
    assert_eq!(
        sending_preferences.mime_type,
        pinned_keys.mime_type.unwrap().into()
    );

    let sending_preferences = SendPreferences::new(
        api_keys,
        None,
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    )
    .expect("should be able to extract sending preferences");

    assert!(
        sending_preferences.encrypt
            && sending_preferences.sign
            && !sending_preferences.is_selected_key_pinned
            && !sending_preferences.encryption_disabled
    );
    assert_eq!(
        sending_preferences.selected_key.unwrap().key_fingerprint(),
        expected_key.key_fingerprint()
    );
    assert_eq!(
        sending_preferences.mime_type,
        composer_preference.composer_body_mime_type.into()
    );
}

#[test]
fn test_sending_preferences_internal_without_e2ee() {
    let pgp = new_pgp_provider();
    let expected_key = pgp
        .public_key_import(TEST_KEY, DataEncoding::Armor)
        .unwrap();
    let pinned_keys = create_test_pinned_key(&pgp, TEST_KEY);
    let api_keys = create_test_public_key(&pgp, false);
    let mail_setting = CryptoMailSettings {
        pgp_scheme: PGPScheme::PGPMime,
        sign: true,
    };

    let composer_preference = ComposerPreference::new(EmailMimeType::Html);

    let sending_preferences = SendPreferences::new(
        api_keys.clone(),
        Some(pinned_keys.clone()),
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    )
    .expect("should be able to extract sending preferences");

    assert!(
        !sending_preferences.encrypt
            && !sending_preferences.sign
            && sending_preferences.is_selected_key_pinned
            && sending_preferences.encryption_disabled
    );
    assert_eq!(
        sending_preferences.selected_key.unwrap().key_fingerprint(),
        expected_key.key_fingerprint()
    );
    assert_eq!(sending_preferences.pgp_scheme, PackageCryptoType::Cleartext);
    assert_eq!(
        sending_preferences.mime_type,
        pinned_keys.mime_type.unwrap().into()
    );

    let sending_preferences = SendPreferences::new(
        api_keys,
        None,
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    )
    .expect("should be able to extract sending preferences");

    assert!(
        !sending_preferences.encrypt
            && !sending_preferences.sign
            && !sending_preferences.is_selected_key_pinned
            && sending_preferences.encryption_disabled
    );
    assert_eq!(
        sending_preferences.selected_key.unwrap().key_fingerprint(),
        expected_key.key_fingerprint()
    );
    assert_eq!(
        sending_preferences.mime_type,
        composer_preference.composer_body_mime_type.into()
    );
}

#[test]
fn test_sending_preferences_external() {
    let pgp = new_pgp_provider();
    let expected_key = pgp
        .public_key_import(TEST_KEY, DataEncoding::Armor)
        .unwrap();
    let mut pinned_keys = create_test_pinned_key(&pgp, TEST_KEY);
    let mut api_keys = create_test_public_key_external(&pgp);
    let mut mail_setting = CryptoMailSettings {
        pgp_scheme: PGPScheme::PGPMime,
        sign: true,
    };

    let composer_preference = ComposerPreference::new(EmailMimeType::Text);

    pinned_keys.encrypt_to_pinned = Some(true);

    let sending_preferences = SendPreferences::new(
        api_keys.clone(),
        Some(pinned_keys.clone()),
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    )
    .expect("should be able to extract sending preferences");

    assert!(
        sending_preferences.encrypt
            && sending_preferences.sign
            && sending_preferences.is_selected_key_pinned
            && !sending_preferences.encryption_disabled
    );
    assert_eq!(
        sending_preferences.selected_key.unwrap().key_fingerprint(),
        expected_key.key_fingerprint()
    );
    assert_eq!(sending_preferences.pgp_scheme, PackageCryptoType::PgpMime);
    assert_eq!(sending_preferences.mime_type, PackageMimeType::Multipart);

    api_keys.unverified = None;

    let sending_preferences = SendPreferences::new(
        api_keys.clone(),
        Some(pinned_keys.clone()),
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    )
    .expect("should be able to extract sending preferences");

    assert!(
        sending_preferences.encrypt
            && sending_preferences.sign
            && sending_preferences.is_selected_key_pinned
            && !sending_preferences.encryption_disabled
    );

    let sending_preferences = SendPreferences::new(
        api_keys.clone(),
        None,
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    )
    .expect("should be able to extract sending preferences");

    assert!(
        !sending_preferences.encrypt
            && sending_preferences.sign
            && !sending_preferences.is_selected_key_pinned
            && !sending_preferences.encryption_disabled
    );
    assert_eq!(sending_preferences.pgp_scheme, PackageCryptoType::ClearMime);
    assert_eq!(sending_preferences.mime_type, PackageMimeType::Multipart);

    mail_setting.sign = false;

    let sending_preferences = SendPreferences::new(
        api_keys.clone(),
        None,
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    )
    .expect("should be able to extract sending preferences");

    assert!(
        !sending_preferences.encrypt
            && !sending_preferences.sign
            && !sending_preferences.is_selected_key_pinned
            && !sending_preferences.encryption_disabled
    );
    assert_eq!(sending_preferences.pgp_scheme, PackageCryptoType::Cleartext);
    assert_eq!(
        sending_preferences.mime_type,
        composer_preference.composer_body_mime_type.into()
    );

    let sending_preferences = SendPreferences::new(
        api_keys.clone(),
        None,
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        ComposerPreference {
            encrypt_to_outside: true,
            composer_body_mime_type: EmailMimeType::Text,
        },
    )
    .expect("should be able to extract sending preferences");

    assert_eq!(
        sending_preferences.pgp_scheme,
        PackageCryptoType::EncryptedOutside
    );
    assert_eq!(sending_preferences.mime_type, EmailMimeType::Text.into());
}

#[test]
fn test_sending_preferences_user_warning() {
    let pgp = new_pgp_provider();
    let pinned_keys = create_test_pinned_key(&pgp, TEST_VERIFICATION_KEY);
    let mut api_keys = create_test_public_key(&pgp, true);
    let mail_setting = CryptoMailSettings {
        pgp_scheme: PGPScheme::PGPMime,
        sign: true,
    };

    let composer_preference = ComposerPreference::new(EmailMimeType::Text);

    let sending_preferences = SendPreferences::new(
        api_keys.clone(),
        Some(pinned_keys.clone()),
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        composer_preference,
    );

    assert!(matches!(
        sending_preferences,
        Err(EncryptionPreferencesError::PinnedKeyNotProvidedByAPI(_))
    ));

    api_keys.address.keys.clear();
    let mut pinned_keys = create_test_pinned_key(&pgp, EXPIRED_KEY);
    pinned_keys.encrypt_to_pinned = Some(true);
    let sending_preferences = SendPreferences::new(
        api_keys,
        Some(pinned_keys.clone()),
        UnixTimestamp::new(1_723_459_962),
        &mail_setting,
        ComposerPreference::default(),
    );

    assert!(matches!(
        sending_preferences,
        Err(EncryptionPreferencesError::ExternalUserNoValidPinnedKey(
            _,
            _,
            _,
            _
        ))
    ));
}

#[test]
fn test_sending_preferences_own() {
    let pgp = new_pgp_provider();
    let address_keys = create_test_decrypted_address_key(&pgp);
    let expected_key = &address_keys.first().unwrap().public_key;

    let mail_setting = CryptoMailSettings {
        pgp_scheme: PGPScheme::PGPMime,
        sign: true,
    };

    let composer_preference = ComposerPreference {
        encrypt_to_outside: false,
        composer_body_mime_type: EmailMimeType::Html,
    };

    let sending_preferences = SendPreferences::new_for_self(
        false,
        &address_keys,
        UnixTimestamp::new(1_723_459_962),
        mail_setting,
        ComposerPreference {
            encrypt_to_outside: false,
            composer_body_mime_type: EmailMimeType::Html,
        },
    )
    .expect("should be able to extract sending preferences");

    assert!(
        sending_preferences.encrypt
            && sending_preferences.sign
            && !sending_preferences.is_selected_key_pinned
            && !sending_preferences.encryption_disabled
    );
    assert_eq!(
        sending_preferences.selected_key.unwrap().key_fingerprint(),
        expected_key.key_fingerprint()
    );
    assert_eq!(
        sending_preferences.pgp_scheme,
        PackageCryptoType::ProtonMail
    );
    assert_eq!(
        sending_preferences.mime_type,
        composer_preference.composer_body_mime_type.into()
    );
}

fn create_test_pinned_key<T: PGPProviderSync>(
    provider: &T,
    test_key: &str,
) -> PinnedPublicKeys<T::PublicKey> {
    let key = provider
        .public_key_import(test_key, DataEncoding::Armor)
        .unwrap();
    PinnedPublicKeys {
        pinned_keys: vec![key],
        encrypt_to_pinned: Some(true),
        encrypt_to_untrusted: Some(true),
        sign: Some(true),
        scheme: None,
        mime_type: Some(EmailMimeType::Html),
        contact_signature_verified: true,
        signature_timestamp: None,
    }
}

fn create_test_public_key<T: PGPProviderSync>(
    provider: &T,
    supports_encryption: bool,
) -> PublicAddressKeys<T::PublicKey> {
    let flags = if supports_encryption {
        KeyFlag::from(1_u32 | 2) // not-compromised | not-obsolete
    } else {
        KeyFlag::from(1_u32 | 2 | 4) // not-compromised | not-obsolete | email-no-encrypt
    };
    let address_keys = vec![APIPublicKey {
        source: APIPublicKeySource::Proton,
        flags,
        primary: true,
        public_key: TEST_KEY.into(),
    }];
    let skl = SignedKeyList {
        min_epoch_id: Some(837),
        max_epoch_id: Some(1407),
        expected_min_epoch_id: None,
        data: Some("Data".into()),
        obsolescence_token: None,
        signature: Some(SKLSignature::from("signature")),
        revision: 31,
    };
    let address_key_keygroup = APIPublicAddressKeyGroup {
        keys: address_keys,
        signed_key_list: Some(skl),
    };
    let api_keys = APIPublicAddressKeys {
        address_keys: address_key_keygroup,
        catch_all_keys: None,
        unverified_keys: None,
        warnings: vec![String::from("this is a warning")],
        proton_mx: true,
        is_proton: false,
    };
    api_keys.import(provider).unwrap()
}

fn create_test_public_key_external<T: PGPProviderSync>(
    provider: &T,
) -> PublicAddressKeys<T::PublicKey> {
    let wkd_key = vec![APIPublicKey {
        source: APIPublicKeySource::WKD,
        flags: KeyFlag::from(3_u32),
        primary: true,
        public_key: TEST_KEY.into(),
    }];
    let address_key_keygroup = APIUnverifiedPublicAddressKeyGroup { keys: wkd_key };
    let api_keys = APIPublicAddressKeys {
        address_keys: APIPublicAddressKeyGroup {
            keys: Vec::new(),
            signed_key_list: None,
        },
        catch_all_keys: None,
        unverified_keys: Some(address_key_keygroup),
        warnings: vec![String::from("this is a warning")],
        proton_mx: true,
        is_proton: false,
    };
    api_keys.import(provider).unwrap()
}

fn create_test_decrypted_address_key<T: PGPProviderSync>(
    provider: &T,
) -> Vec<DecryptedAddressKey<T::PrivateKey, T::PublicKey>> {
    let private_key = provider
        .private_key_import(
            PRIVATE_KEY,
            PRIVATE_KEY_PASSWORD.as_bytes(),
            DataEncoding::Armor,
        )
        .unwrap();
    let public_key = provider.private_key_to_public_key(&private_key).unwrap();
    vec![DecryptedAddressKey {
        id: KeyId::from(
            "G8URRzoYaBW6mSPQjbbo2yYgwI828DVcEs8dDRKxByd1A_qSRYF49TOtw_m4wvDGb76M-r3AVdXuDzSHObR5hQ==",
        ),
        private_key,
        public_key,
        flags: KeyFlag::from(3_u32),
        primary: true,
        is_v6: false,
    }]
}
