//! This integration test demonstrates how to implement Proton's email-sending feature
//! using the tools and utilities provided by this crate. It serves as an example of
//! composing, encrypting, and sending emails with Proton.
//!
//! Note the example here is not complete.
//!
//! The tests only validate body decryption on the recipient side and no attachment decryption.
use core::str;
use std::collections::HashMap;

use proton_crypto_account::keys::{
    APIPublicAddressKeyGroup, APIPublicAddressKeys, APIPublicKey, APIPublicKeySource,
    EmailMimeType, KeyFlag, PGPScheme, PublicAddressKeys, SKLDataJson, SKLSignature, SignedKeyList,
    UnlockedAddressKey,
};

use proton_crypto_inbox::{
    attachment::{
        AttachmentEncryptedSignature, AttachmentSignature, DecryptableAttachment, KeyPackets,
    },
    keys::{
        ComposerPreference, CryptoMailSettings, InboxSessionKey, KeyPacket, PackageCryptoType,
        SendPreferences, SessionKeyExposed,
    },
    message::{
        DecryptableMessage, DecryptedBody, GettablePGPMessage, SessionKeyAndDataPacketsExtractable,
        packages::{EncryptablePackage, EncryptedPackageBody, PackageMimeType},
        to_sanitized_string,
    },
    proton_crypto::{
        crypto::{
            ArmorerSync, DataEncoding, Decryptor, DecryptorSync, PGPProviderSync,
            SessionKeyAlgorithm, UnixTimestamp, VerifiedData,
        },
        new_pgp_provider,
    },
};

use proton_crypto_inbox_mime::{MimeProcessor, ProcessMime, write::InboxMimeBuilder};

mod common;

/// Sender private address key.
const ADDRESS_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZug8RxYJKwYBBAHaRw8BAQdAOqPmuLm/d1IsfczuKeov4BDpycbGl5e/
M+Iopk7Gasv+CQMIR3RuaTTi8EVgAAAAAAAAAAAAAAAAAAAAADnmE3EtwYYE
uV7MERdHt83vdjjNqz0qNCB71iQRg/gBV+BW3UhKGfo39i+Iu5MtnjablRvb
A80vcnVzdF90ZXN0QHByb3Rvbi5ibGFjayA8cnVzdF90ZXN0QHByb3Rvbi5i
bGFjaz7CjAQQFgoAPgWCZug8RwQLCQcICZB/UiP0sHg7pQMVCAoEFgACAQIZ
AQKbAwIeARYhBA0dwi+mwfhhzXbTrH9SI/SweDulAAA+kAD+OZIV7yciD3SN
Zo8FqbB3BqE47CAiTfW1C8keP65tpdcBAJWP4qFCeERlTVhJcDMlDttjfyZd
8BjL6S6FMMQOgtwDx4sEZug8RxIKKwYBBAGXVQEFAQEHQHsOSjw9I2jaD0Oc
zPZhUEeel3B6e828liM/LrzCeMALAwEIB/4JAwjMktb9VzI6fGAAAAAAAAAA
AAAAAAAAAAAAwyAqz5bpwzsfbVaPAQHJTcmUMAZ/mjs7yOjUYKESCxePFVUD
BnbszSdixRhjotDizcuQv6zmwngEGBYKACoFgmboPEcJkH9SI/SweDulApsM
FiEEDR3CL6bB+GHNdtOsf1Ij9LB4O6UAAFNxAQCaP5G0R077BRKHjv0m4R5B
iog/MN7XjI1TdQNWw/xLHgD/dZnRTJ0QssXiFGJF1QenFw4qZIhqhLft+sN1
RlBvVQw=
=DEvB
-----END PGP PRIVATE KEY BLOCK-----
";

/// Recipient private address key to validate test output.
const RECIPIENT_INTERNAL_DECRYPTION_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZsNYwBYJKwYBBAHaRw8BAQdAfgCr5GL1OaFneBvtkCLKtXfxXhBuOu7w
ErbqLyzOqO3+CQMI72Hv5qoR4EBgS8tLgA0S8ulvFHaRiJwQ2gIJeHZnXLut
xqJJUcTjmVCsfHEj4IpHLtWrQJAipwN5YQGlQGb6/kPTufO8285ZAU/Nfd2U
2s0pbHVidXFhQHByb3Rvbi5ibGFjayA8bHVidXFhQHByb3Rvbi5ibGFjaz7C
jAQQFgoAPgWCZsNYwAQLCQcICZCMeGsCewp8JQMVCAoEFgACAQIZAQKbAwIe
ARYhBIC5mtq+YeltdGL+Yox4awJ7CnwlAAD7VQD/arWGM9zBUL2ORQ9XsEmT
Ak1MGpKBtaOBojMFtZvoUz8A/A8DBVOZjAWoS5xhI+2mEQ+Ba9LJJZ0rpvEk
aEGwxYUGx4sEZsNYwBIKKwYBBAGXVQEFAQEHQEGib3H/MU1efqhDNwbNK7CB
yIpSultvip67P/sqfJM2AwEIB/4JAwjvPmZDu8GETmBCEZmu9iu7bOz6xV/c
C4MNWNyKY+LSod/1MQYvrloo0LPw7IAWCtEvYUlwFpeFcgcT1mP0fC4KZpaN
SCkA+MduQzIVSiv1wngEGBYKACoFgmbDWMAJkIx4awJ7CnwlApsMFiEEgLma
2r5h6W10Yv5ijHhrAnsKfCUAALLKAPsE2U3OfszpULLyWnu3C88OSc0/bWgL
y0u1k8OmDane8wD+Nn/GEgLHtcgx6xhyZVKc3lmJD49u0XDREUsCQXeocQo=
=pN6C
-----END PGP PRIVATE KEY BLOCK-----
";

/// Recipient private address key v6 to validate test output.
const RECIPIENT_INTERNAL_DECRYPTION_KEY_V6: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xX0GZ1HqGRsAAAAgsLRL1RXfGij8JJAsHHhYE78buH4smu2/7Ht9X6NXnRb+
HQkLAwiiGHXGG8+W9WAAAAAAAAAAAAAAAAAAAAAAwtnL2dF74WBQi3MIOxWV
zCDq3cc39mt8gsEauYeA7rccYtdct3sjDLQ7q0g3V8X/ZXafE8KtBh8bCgAA
AD4FgmdR6hkDCwkHBRUKCA4MBBYAAgECmwMCHgEioQZB9IyvzsJ+jWGlvVgM
gFy1qqF1QbuEL8Q/sVjB4SOhEwAAAAAKzCBjCWPa05vTQuXrwRXOItXW2xHF
ZyztjkAMPMOHmYFrwLKwLAHjIYX//8cbpbmSJ11tj1/Il72UJXAnjdo9Ha3S
sXsna3SH+uhUWMu/ox/0GQh9gzDz6QiUYtqzyfBHgwfNPXBxYy1hY2NvdW50
LWtleXNAcHJvdG9uLmJsYWNrIDxwcWMtYWNjb3VudC1rZXlzQHByb3Rvbi5i
bGFjaz7CmwYTGwoAAAAsBYJnUeoZAhkBIqEGQfSMr87Cfo1hpb1YDIBctaqh
dUG7hC/EP7FYweEjoRMAAAAAUfUgdNPsyt8s63D0hSYh5MyhIOSAgLk6Itqc
KWd2BIrRmFe8uLsmgThttHyMIjFQsRnW3xVXVh1Ledh5yCpacGQGx5qH/OWt
N9Ez3vjEHoIbfhzVTfc0xcQ3hMZ7KU2m37gFx30GZ1HqGRkAAAAgvY33Bo60
cRmgiNhZf3q6LEhacrNrHuJgUb0ZXGX/gHX+HQkLAwjZk27ShJ8ooWAAAAAA
AAAAAAAAAAAAAAAAuBxzVs8E2iNjnkeKTQmcWMKmxwUvzrBCSvDhgDc93yvK
M+F014Qa4wJTnE9XD2BHr3FBi8KbBhgbCgAAACwFgmdR6hkCmwwioQZB9Iyv
zsJ+jWGlvVgMgFy1qqF1QbuEL8Q/sVjB4SOhEwAAAAAdZCClLiEqTQJeJNou
+8kBM7X9vvpGJMJZWNd1YwsF4MfPFo6WaAK4jWUUO6FJBP/inoyI/WMM8UG4
z8OEpXTwjkn+fOTv0gNTMjhAJCCVKpj7BTDGFlswL0zJt6pWzLnz1gE=
-----END PGP PRIVATE KEY BLOCK-----
";

/// Recipient private address key to validate test output.
const RECIPIENT_EXTERNAL_DECRYPTION_KEY: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZf15lRYJKwYBBAHaRw8BAQdArPz06hKiOUYSVs6dbHpKSh63bW5/QyIFqRvJ5wOALJn+CQMI
Dcn5dni4PZ343n8joQ0R+ZBJ8Dtc+HHGvBz0THwLtLfHR9rRY2YUprZEmah1wZI62Iyretg9ZLCn
KHNlVu/xKvQEFo+EpllafoUlj2hwtc0yTHVrYXMgQnVya2hhbHRlciA8a2V5dHJhbnNwYXJlbmN5
bWFpbGVyQGdtYWlsLmNvbT7CjwQTFggANxYhBI0R/ncVToQyZmzjekbw+nCNM2IgBQJl/XmVBQkF
o5qAAhsDBAsJCAcFFQgJCgsFFgIDAQAACgkQRvD6cI0zYiBpfQEA0oasQ2Azudum3m45F9pPkkvH
r3mrVGAoN62bptCEHX8BAMe9jeiFr55ps37Ipw5YqW+82ltttjOO6eeuZ+bt34cMx4sEZf15lRIK
KwYBBAGXVQEFAQEHQCrgpEW7h39eZHZQQaOxd5YJa+8pSTOwJH/BEOfwT+VFAwEIB/4JAwj0HdFG
Lq4i8/gxphOlOqqYTDXVHYCalE4jrGV+e1HuaIpq4DmKQxwbz7LUzdjH/kdbETaesKuHFenopdRQ
+e4GAybCo+tlnn3RjAE+/+Anwn4EGBYIACYWIQSNEf53FU6EMmZs43pG8PpwjTNiIAUCZf15lQUJ
BaOagAIbDAAKCRBG8PpwjTNiIBM3AQD6hJsjnJ2827BiWmebhRANznxh+2pxINsRG32dI8QIdAD/
Z7DviakK1VuZt2hluDETBUY5wtgyB7YU4TUDMNW2aQ0=
=EZM6
-----END PGP PRIVATE KEY BLOCK-----
";

const EXPECTED_MESSAGE_INTERNAL: &str = r#"<div style="font-family: Arial, sans-serif; font-size: 14px;">Test</div><div style="font-family: Arial, sans-serif; font-size: 14px;"><br></div>
<div class="protonmail_signature_block" style="font-family: Arial, sans-serif; font-size: 14px;">
    <div class="protonmail_signature_block-user protonmail_signature_block-empty">

            </div>

            <div class="protonmail_signature_block-proton">
        Sent with <a target="_blank" href="https://proton.me/mail/home">Proton Mail</a> secure email.
    </div>
</div>
"#;

const EXPECTED_MESSAGE_EXTERNAL: &str = r#"<div style="font-family: Arial, sans-serif; font-size: 14px;">Test</div><div style="font-family: Arial, sans-serif; font-size: 14px;"><br></div>
<div class="protonmail_signature_block" style="font-family: Arial, sans-serif; font-size: 14px;">
    <div class="protonmail_signature_block-user protonmail_signature_block-empty">

            </div>

            <div class="protonmail_signature_block-proton">
        Sent with <a target="_blank" href="https://proton.me/mail/home">Proton Mail</a> secure email.
    </div>
</div>
"#;

/// Simulates sending an email internally from a single sender to a single recipient.
/// Assumes the email draft content is in HTML format and the recipient wants HTML-formatted emails.
#[test]
fn send_internal() {
    let pgp = new_pgp_provider();

    let (mail_settings, composer_preferences, draft, sender_keys) =
        send_logic::setup_test_environment(&pgp, models::DraftMessage::load_internal);

    let recipient_preferences = send_logic::create_send_preferences(
        &pgp,
        mail_settings,
        composer_preferences,
        recipient_keys::load_recipient_public_key_internal,
        PackageMimeType::Html,
    );

    let encrypted_body =
        EncryptedPackageBody::new_with_draft(&pgp, &draft, PackageMimeType::Html, &sender_keys)
            .expect("Failed to create encrypted package body");

    assert_eq!(draft.mime_type, "text/html");

    let package = send_logic::process_package(
        &pgp,
        draft.to_list.first().unwrap(),
        &draft.attachments,
        &encrypted_body,
        &sender_keys,
        recipient_preferences,
    );

    validate_decryption(
        &package,
        RECIPIENT_INTERNAL_DECRYPTION_KEY,
        EXPECTED_MESSAGE_INTERNAL,
    );
}

/// Simulates sending an email internally from a single sender to a single recipient with v6 keys.
/// Assumes the email draft content is in HTML format and the recipient wants HTML-formatted emails.
#[test]
fn send_internal_v6() {
    let pgp = new_pgp_provider();

    let (mail_settings, composer_preferences, draft, sender_keys) =
        send_logic::setup_test_environment(&pgp, models::DraftMessage::load_internal);

    let recipient_preferences = send_logic::create_send_preferences(
        &pgp,
        mail_settings,
        composer_preferences,
        recipient_keys::load_recipient_public_key_internal_v6,
        PackageMimeType::Html,
    );

    let encrypted_body =
        EncryptedPackageBody::new_with_draft(&pgp, &draft, PackageMimeType::Html, &sender_keys)
            .expect("Failed to create encrypted package body");

    assert_eq!(draft.mime_type, "text/html");

    let package = send_logic::process_package(
        &pgp,
        draft.to_list.first().unwrap(),
        &draft.attachments,
        &encrypted_body,
        &sender_keys,
        recipient_preferences,
    );

    validate_decryption(
        &package,
        RECIPIENT_INTERNAL_DECRYPTION_KEY_V6,
        EXPECTED_MESSAGE_INTERNAL,
    );
}

/// Make sure we can send an email to a self-owned, internal address with
/// encryption enabled.
///
/// This happens when you send an email from `foo@protonmail.com` to
/// `foo@pm.me`, for instance.
#[test]
fn send_to_internal_encrypted_self_address() {
    let pgp = new_pgp_provider();

    let (mail_settings, composer_preferences, draft, sender_keys) =
        send_logic::setup_test_environment(&pgp, models::DraftMessage::load_internal);

    let recipient_preferences = SendPreferences::new_for_self(
        false,
        &sender_keys,
        UnixTimestamp::new(1_726_502_569),
        mail_settings,
        composer_preferences,
    )
    .unwrap();

    let encrypted_body =
        EncryptedPackageBody::new_with_draft(&pgp, &draft, PackageMimeType::Html, &sender_keys)
            .expect("Failed to create encrypted package body");

    assert!(!recipient_preferences.encryption_disabled);
    assert_eq!(draft.mime_type, "text/html");

    let package = send_logic::process_package(
        &pgp,
        draft.to_list.first().unwrap(),
        &draft.attachments,
        &encrypted_body,
        &sender_keys,
        recipient_preferences,
    );

    validate_decryption(&package, ADDRESS_KEY, EXPECTED_MESSAGE_INTERNAL);
}

/// Make sure we can send an email to a self-owned, internal address with
/// encryption disabled.
///
/// This happens when you configure forwarding to an external servide provider,
/// e.g. from `foo@pm.me` to `foo@gmail.com`, and then you send a message to
/// `foo@pm.me`.
///
/// Confusingly enough, this actually *does* encrypt the message!
///
/// That's because we're sending a message to ourselves and so it does not get
/// actually forwarded to `foo@gmail.com`, it appears only inside `foo@pm.me`.
///
/// This message would *not* be encrypted if we were sending it to somebody else
/// (to an address we don't own), though; but in here we're testing just the
/// self-sending scenario.
#[test]
fn send_to_internal_unencrypted_self_address() {
    let pgp = new_pgp_provider();

    let (mail_settings, composer_preferences, draft, mut sender_keys) =
        send_logic::setup_test_environment(&pgp, models::DraftMessage::load_internal);

    for key in &mut sender_keys.0 {
        key.flags.set_email_no_encryption();
    }

    let recipient_preferences = SendPreferences::new_for_self(
        false,
        &sender_keys,
        UnixTimestamp::new(1_726_502_569),
        mail_settings,
        composer_preferences,
    )
    .unwrap();

    let encrypted_body =
        EncryptedPackageBody::new_with_draft(&pgp, &draft, PackageMimeType::Html, &sender_keys)
            .expect("Failed to create encrypted package body");

    assert!(!recipient_preferences.encryption_disabled);
    assert_eq!(draft.mime_type, "text/html");

    let package = send_logic::process_package(
        &pgp,
        draft.to_list.first().unwrap(),
        &draft.attachments,
        &encrypted_body,
        &sender_keys,
        recipient_preferences,
    );

    validate_decryption(&package, ADDRESS_KEY, EXPECTED_MESSAGE_INTERNAL);
}

/// Make sure we can send an email to a self-owned, external address with
/// encryption disabled.
///
/// This happens when you create an account on non-mail Proton service - say,
/// Proton VPN - and later you "upgrade" to a Proton Mail account.
///
/// In cases like these, we will have keys for `foo@gmail.com` or whatever, but
/// they cannot be used for mail encryption purposes.
#[test]
fn send_to_external_unencrypted_self_address() {
    let pgp = new_pgp_provider();

    let (mail_settings, composer_preferences, _, mut sender_keys) =
        send_logic::setup_test_environment(&pgp, models::DraftMessage::load_internal);

    for key in &mut sender_keys.0 {
        key.flags.set_email_no_encryption();
    }

    let recipient_preferences = SendPreferences::new_for_self(
        true,
        &sender_keys,
        UnixTimestamp::new(1_726_502_569),
        mail_settings,
        composer_preferences,
    )
    .unwrap();

    assert!(recipient_preferences.encryption_disabled);
}

/// Simulates sending an email from a single internal sender to a single external recipient without keys.
/// Assumes the email draft content is in HTML format and the recipient wants HTML-formatted emails.
#[test]
fn send_external_no_keys() {
    let pgp = new_pgp_provider();

    let (mail_settings, composer_preferences, draft, sender_keys) =
        send_logic::setup_test_environment(&pgp, models::DraftMessage::load_internal);

    let recipient_preferences = send_logic::create_send_preferences(
        &pgp,
        mail_settings,
        composer_preferences,
        recipient_keys::load_recipient_public_key_external_no_keys,
        PackageMimeType::Html,
    );

    let encrypted_body =
        EncryptedPackageBody::new_with_draft(&pgp, &draft, PackageMimeType::Html, &sender_keys)
            .expect("Failed to create encrypted package body");

    assert_eq!(draft.mime_type, "text/html");

    let package = send_logic::process_package(
        &pgp,
        draft.to_list.first().unwrap(),
        &draft.attachments,
        &encrypted_body,
        &sender_keys,
        recipient_preferences,
    );

    validate_decryption(
        &package,
        RECIPIENT_INTERNAL_DECRYPTION_KEY,
        EXPECTED_MESSAGE_INTERNAL,
    );
}

/// Simulates sending an email from a single internal sender to a single external recipient with `OpenPGP` keys.
#[test]
fn send_external_mime() {
    let pgp = new_pgp_provider();

    let (mail_settings, composer_preferences, draft, sender_keys) =
        send_logic::setup_test_environment(&pgp, models::DraftMessage::load_external);

    let recipient_preferences = send_logic::create_send_preferences(
        &pgp,
        mail_settings,
        composer_preferences,
        recipient_keys::load_recipient_public_key_external,
        PackageMimeType::Multipart,
    );

    assert_eq!(draft.mime_type, "text/html");

    let mime_body = send_logic::build_mime(&draft, &pgp, &sender_keys);

    let primary = sender_keys
        .primary_for_mail()
        .expect("Primary should be there");

    let encrypted_body = mime_body
        .package_body_encrypt(&pgp, &primary)
        .expect("Package encryption failed");

    let package = send_logic::process_package(
        &pgp,
        draft.to_list.first().unwrap(),
        &draft.attachments,
        &encrypted_body,
        &sender_keys,
        recipient_preferences,
    );

    validate_decryption(
        &package,
        RECIPIENT_EXTERNAL_DECRYPTION_KEY,
        EXPECTED_MESSAGE_EXTERNAL,
    );
}

/// Simulates sending an email from a single internal sender to a single external recipient with `OpenPGP` keys but sign only.
#[test]
fn send_external_mime_sign_only() {
    let pgp = new_pgp_provider();

    let (mail_settings, composer_preferences, draft, sender_keys) =
        send_logic::setup_test_environment(&pgp, models::DraftMessage::load_internal);

    let mut recipient_preferences = send_logic::create_send_preferences(
        &pgp,
        mail_settings,
        composer_preferences,
        recipient_keys::load_recipient_public_key_external,
        PackageMimeType::Multipart,
    );
    recipient_preferences.encrypt = false;
    recipient_preferences.pgp_scheme = PackageCryptoType::ClearMime;

    assert_eq!(draft.mime_type, "text/html");

    let mime_body = send_logic::build_mime(&draft, &pgp, &sender_keys);

    let primary = sender_keys
        .primary_for_mail()
        .expect("Primary should be there");

    let encrypted_body = mime_body
        .package_body_encrypt(&pgp, &primary)
        .expect("Package encryption failed");

    let package = send_logic::process_package(
        &pgp,
        draft.to_list.first().unwrap(),
        &draft.attachments,
        &encrypted_body,
        &sender_keys,
        recipient_preferences,
    );

    validate_decryption(
        &package,
        RECIPIENT_EXTERNAL_DECRYPTION_KEY,
        EXPECTED_MESSAGE_EXTERNAL,
    );
}

/// Contains all test models for the send email request.
mod send_request {

    use proton_crypto_inbox::attachment::Base64AttachmentEncryptedSignature;

    use super::*;

    #[derive(Debug, Default, PartialEq, Eq, Clone)]
    pub struct SendRequest {
        pub expiration_time: u64,
        pub expires_in: u64,
        pub auto_save_contacts: u8,
        pub delay_seconds: u64,
        pub delivery_time: u64,
        pub packages: Vec<SendPackage>,
    }

    #[derive(Debug, Default, PartialEq, Eq, Clone)]
    pub struct SendPackage {
        pub addresses: HashMap<String, SendAddress>,
        pub mime_type: PackageMimeType,
        pub package_type: i32,
        pub body_key: Option<ExposedKey>,
        pub attachment_keys: HashMap<String, ExposedKey>,
        pub body: Option<Vec<u8>>,
    }

    #[derive(Debug, Default, PartialEq, Eq, Clone)]
    pub struct SendAddress {
        pub address_type: PackageCryptoType,
        pub body_key_packet: Option<KeyPacket>,
        pub attachment_key_packets: Option<HashMap<String, KeyPacket>>,
        pub attachment_enc_signatures: Option<HashMap<String, Base64AttachmentEncryptedSignature>>,
        pub signature: Option<PackageSignaturesMode>,
        pub token: Option<String>,
        pub enc_token: Option<String>,
        pub auth: Option<()>,
        pub password_hint: Option<String>,
    }

    #[derive(Debug, PartialEq, Eq, Clone)]
    pub struct ExposedKey {
        pub key: SessionKeyExposed,
        pub algorithm: SessionKeyAlgorithm,
    }

    impl From<InboxSessionKey> for ExposedKey {
        fn from(value: InboxSessionKey) -> Self {
            Self {
                key: value.expose_secret(),
                algorithm: value.algorithm(),
            }
        }
    }

    #[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
    pub enum PackageSignaturesMode {
        #[default]
        None = 0,
        Attachments = 1,
    }

    impl From<bool> for PackageSignaturesMode {
        fn from(value: bool) -> Self {
            if value { Self::Attachments } else { Self::None }
        }
    }
}

/// Contains all test domain models.
mod models {
    use proton_crypto_inbox_mime::Disposition;

    use super::*;

    #[derive(Debug, PartialEq, Eq, Clone)]
    pub struct Attachment {
        pub id: String,
        pub name: String,
        pub size: u64,
        pub mime_type: String,
        pub disposition: Disposition,
        pub key_packets: KeyPackets,
        pub signature_key_packets: Option<String>,
        pub headers: HashMap<String, String>,
        pub signature: Option<AttachmentSignature>,
        pub enc_signature: Option<AttachmentEncryptedSignature>,
    }

    impl Attachment {
        pub fn test_internal() -> Self {
            Self {
                id: "hEKBJhne3BhMK3lzPECCcnzQt_eNsb6mrXtMIV76ksZKpKR51OS4JO7YLunztzMHnSksMuC7Wdf-3LpK_4KUVA==".to_owned(),
                name: "attachment.rs".to_owned(),
                size: 56,
                mime_type: "application/rls-services+xml".to_owned(),
                disposition: Disposition::Attachment,
                key_packets: KeyPackets::from("wV4DYQP2QwXoAwsSAQdA/l4bd0trb7bSHzVVQeHpJUZaY12y8vnQt4iFx7/jRFcwyBLVzQdaNTIlnr+g+A/tsiniYqq45fXBvssCVAHwloD9oy7hS5RNvmwwBm3GoDou"),
                signature_key_packets: None,
                headers: HashMap::from([("content-disposition".to_owned(), "attachment".to_owned())]),
                signature: Some(AttachmentSignature::from("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwnUEABYKACcFgmboQxkJkH9SI/SweDulFiEEDR3CL6bB+GHNdtOsf1Ij9LB4\nO6UAAGibAP9+p6q2iglDzamD/7ZnFtAoDQ4Gll997ALFbLYJeA4jvgD/YsSF\nxs/OO97VNPEnDjwNDz1Hh5lwAS7bIwBAJiVn4gg=\n=3x2a\n-----END PGP SIGNATURE-----\n")),
                enc_signature: Some(AttachmentEncryptedSignature::from("-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DYQP2QwXoAwsSAQdAw0wA5OzMS7Ddj0Uj/6Akgqh5EXk99WMCQD6MDgXx\n/gkwvuNdwaOewp2fMZ7qULboTl5HY6qUQeK0muFCw8tETuELXEbgS/o7kQwP\nRVGZb8d20qgB/HR/CxrnJDtOlizfe7m8ikRhAhVsBnIQo4idSLHhoEg2c4IP\nhhxugV/k8WKyBies2ScED4TmHNI2K6/RECjBW/GbHE0mK9Z824Ktf4aabRcq\nUHOabf5j3fS/fOLsXn/LHjVCNNj0qvDk8LKFf/BFcgHQ+EGD/m2vuVmPSk4+\na4E2hWqf4MZ7prlhFSd6ibWQ4RTa10xk0QfzCeNduE7S745vikUnIl8=\n=+XY+\n-----END PGP MESSAGE-----\n")),
            }
        }

        pub fn test_external() -> Self {
            Self {
                id: "2N1wVl4-LH_O5tv5UVPuR_SEUNvIGc8GJviaqAgf76rZ0NJ6bS3_jVqB5XD2HQtfbcL3n6Ye9MEJ54zvzjI_ow==".to_owned(),
                name: "attachment.rs".to_owned(),
                size: 56,
                mime_type: "application/rls-services+xml".to_owned(),
                disposition: Disposition::Attachment,
                key_packets:  KeyPackets::from("wV4DYQP2QwXoAwsSAQdASUaO79aVst+w3y3iwG3QEh3gGgpon4cAsy3k24tq6DYwtz8M89uaRCUno7aoRqyFgwAtg3OcOCZBeZS5RkTu7FKy10tDIuaPG25glYBx7h2h"),
                signature_key_packets: None,
                headers: HashMap::from([("content-disposition".to_owned(), "attachment".to_owned())]),
                signature: Some(AttachmentSignature::from("-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwnUEABYKACcFgmboRdUJkH9SI/SweDulFiEEDR3CL6bB+GHNdtOsf1Ij9LB4\nO6UAAFvoAP9ZIzZO4lSK3XMMhmkvvBRr5xf3X4PWCxQG80a6klyikAEA4rmX\n0hgPK587vfjohtknBTIU5eHGa+f5UNgTefaiKwg=\n=gWZg\n-----END PGP SIGNATURE-----\n")),
                enc_signature: Some(AttachmentEncryptedSignature::from("-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DYQP2QwXoAwsSAQdAdOfuCXf/kS1vrt5AyYGXr+b8lC/i/gUKhaPc38WX\nXVcw3svAX56jnonCpBTCYmXfJKyVOZwDfbhKckQb3N8X3wlfwjlyihQ8Of6R\nA4nrP6Jd0qgBB2DyZVjpUsio6U/EKgUn6xKFMFvNMr1O22/QJTlBx3lPpptZ\n+dvZsnlxGxJN9dlcp9Jbzg+TxNkZKfa5Wmtjkq1mLbEsAAtYbXEWFMdNZJ1B\ntUhF7SUMCBZYjxzSEgfSMTtChLXv0AtXCemVRZ9jC85gbWFowLY5vksMJwcI\nHmYmTs46CvjKVNZd1kl3luB+fY9dFsvSJPxg7WeFeZuGwx1fdfd1ff4=\n=P7x4\n-----END PGP MESSAGE-----\n")),
            }
        }
    }

    impl DecryptableAttachment for Attachment {
        fn attachment_key_packets(&self) -> &KeyPackets {
            &self.key_packets
        }

        fn attachment_signature(&self) -> Option<&AttachmentSignature> {
            self.signature.as_ref()
        }

        fn attachment_encrypted_signature(&self) -> Option<&AttachmentEncryptedSignature> {
            self.enc_signature.as_ref()
        }
    }

    #[derive(Debug, Default, PartialEq, Eq, Clone)]
    pub struct DraftMessage {
        pub id: String,
        pub subject: String,
        pub sender: String,       // Simplified
        pub to_list: Vec<String>, // Simplified
        pub num_attachments: u8,
        pub body: String,
        pub mime_type: String,
        pub attachments: Vec<Attachment>,
    }

    impl DraftMessage {
        pub fn load_internal() -> Self {
            Self {
                id: "yIcE6naErdMnomTQuDHQTJ5hrswuBg0U0pv1z4Zi-BTNhqU2yqD-EyvDmqPy4o5GjAlJK4ESWJ7ptvJ64RKs9g==".to_owned(),
                subject: "Test".to_owned(),
                sender: "rust_test@proton.black".to_owned(),
                to_list: vec!["lubuqa@proton.black".to_owned()],
                num_attachments: 1,
                body: "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DYQP2QwXoAwsSAQdAjxe3G/bl9vhs4vetW5HXbLMNfSoI9n84SjLXow3J\nDlkwKigDXdl0/G1i89AlatUpP200fMoAjxUJBTo7fkD/6QXToE+lPmtnj6Cy\ntBoCnt5V0sIRAc2ycB2aIH1IuOeqy47Pkp2dNP/hLluSwgFZj74RNIj5crWz\nHJfJ8Y7vr0dVt68RZTxiA9dTc+tiJuKsAr5LaW+ctP6lbfr4I/e+sMN8IEcF\nF5yUFcIYTLVs/PXYXhYI8t7vwWyIoTIIVztuBCmjv+aCdG47NNoTKKJv/VK+\nUFR3D3rzgQJ5W2VobRxCRr3pDFpySBHxz1t39hC9KqtXKy4FEp90Hcqn4W1A\nN7DfNBSSRNcdqQTkPeGF6N5dz5VjEprxeTG43sUjR4+VP7H6V7+2vwsQrwpG\nXkh2mKyjFa9l3RLoj9FwNWZcEm8or853iGFlzVmHyPm/qcy5jG8bDHEMc15u\nNTSog7ruVNznyGuUYGgWNIfpeIYm1G9mYtjq36SoJKO5Fb2wY1+PDilJWljc\nnXOLZ83L82UlPeJdvkDODkNCYEvwjHgU83wLdEeW0lukn5BERMZTj+qzVK4H\nIx24iRxkXjocelyn8xCI/jeYGNV2lm+Gf1UMTsCNGztU7+FlvyQ7kqqSDT5s\ntzxCQ32o8f1DjrRzajeJtcNj/cuEI8OURMJwPyOL03/ExSjywfT1/ll4sWcf\nsfgoBPD5ltcqdMLQMzs0evWrTe17aFJexkcByTRP3UPirlfcqPzzksMUfdBn\nYVJROCMdIl+J2sVpN3il0I4ZlbI2L8NujvhHg1blaFo1qdGpTW6lU2lVS0Pf\nSQPbRfjJ2gF+kFgVvT6uDxwhKOV9Qzow8n95zghW8g8DJbC1cOW7Cg5Rujut\nC8JD4/zqGle9pxqpUe0IkOm6zSX4I8ppITSo/BEjbNP/Z5cjzD70XI7UmgTX\nfHSx6ivHxrbuwsKYmF1W1ZycT/z55lJranbTlnaVKg+/9csKdbk7sBKIHedY\n8WIj44JNpHOxWnREri9jcSPY8jwk6v4mORcg0oHHDB98SH7owTizGwlUJ2oU\nALkGzxe5Vm+R0A==\n=Svkq\n-----END PGP MESSAGE-----\n".to_owned(),
                mime_type: "text/html".to_owned(),
                attachments: vec![Attachment::test_internal()],
            }
        }

        pub fn load_external() -> Self {
            Self {
                id: "7ACmFcLcGzrnp3RNkkw3zXndi2iBl8-HH3Ohcb8qGgExPu38LJeLXE9-vc-q_uVJp1G4sakW-ysaZIdPp8VfxA==".to_owned(),
                subject: "Test".to_owned(),
                sender: "rust_test@proton.black".to_owned(),
                to_list: vec!["keytransparencymailer@gmail.com".to_owned()],
                num_attachments: 1,
                body: "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4DYQP2QwXoAwsSAQdAJW/4W9QNzIeKsRP1daJPIGw3TNYMTUpeVVLaKFJ9\n7Bww4I4ivsC+lFcK9xRwsa+48gaQyGPv0bGFMn0auhR2YYFyJEevOlwTJSo7\nhGZhVsSg0sIRAbunANGalWkp8QlDbkysDNS0GiqWbBDjA+pP5aZ1g5+VbsFa\ntaa10DxvcOYyvY4/4/0FPpPkKYvYnvSvfFde7e8CR5V5V0rqmVCR8mR+wbYM\njjkcHcFCGxlhqvWbt2rJZVPTjd5A58jFCoDkpU0ktXYYGCx1rp8eSdqmTmvP\naffj+Y2vUw965OlQyaxua1IcBkAl9Erx2EMEw0AeGfmlJppG56hkWMCXNpg/\n9dP8Z2lOifnYJa5gFuaDFz8wgNAzqKe6DFXjTZAcBW6g0yGUVkv3nzfgQ5SL\n/YrS7d2xJGt1OPAVX2oTMS/grr14xr+wMd7vpoX7CFR6ZQJoPxv/zSFtFrYZ\nszPeyHPSEIFsbnL10H0ddPWlYxhUV01GWGu0ct4+aV5wP24yjxaAHAAItuvO\nnlxDQim0HCQ6FB4eE3J/vXfhVFDMQK4N/YNo/0J02pa789DsEd4blcWH3opJ\nwgTCFeacaXDSubse665YQAbg3eL+xuGZZm7PpbGAMCLXrH2syDaH6S6UJiAq\num3vxhGYXsMSQ+LgAXORR4/Zezfzheu+s+3v0Jkymk53O2fBWBajg8NCJTzY\nKoS0wqkRnPVM+ToYFAqi34hoLTQQXiLOZd2dyMtH3xKAuYqg0ZHAssTy7gkj\nQ581QikuChmiS7UHvv0HrOeTifzQa0LupxANigxwb29dbgLb23hxJXtG/KU8\nqL9iavw4euH5VVpcfjTi7lsFO0NHVm+pZYI070bOjFhlVwl+inv8GdS/s12D\nL66JlQozCPJRfruL2VVWqMeVcAzecNoLt+t1rOxrcoZlLD1VYbjApfCAhdL4\nSOYBkRE1oOMNJV0p5Bbcp3bkxtORXuoQJuT5OvOs88qd+DBU2PY3Fdu2E8yw\nx49j1Cad9o2sgcnHDYaFtF3N9Dnp9kU86zSgEyUs3Ce5hhK1dI+WvjQ4s5Ap\n15LIhxfaXXjiVw==\n=EXcr\n-----END PGP MESSAGE-----\n".to_owned(),
                mime_type: "text/html".to_owned(),
                attachments: vec![Attachment::test_external()],
            }
        }
    }

    impl GettablePGPMessage for DraftMessage {
        fn pgp_message(&self) -> &[u8] {
            self.body.as_bytes()
        }
    }

    impl DecryptableMessage for DraftMessage {
        fn message_is_mime(&self) -> bool {
            &self.mime_type == "multipart/mixed"
        }

        fn message_id(&self) -> Option<&str> {
            Some(&self.id)
        }
    }

    impl SessionKeyAndDataPacketsExtractable for DraftMessage {}

    pub fn load_mail_settings() -> CryptoMailSettings {
        CryptoMailSettings {
            pgp_scheme: PGPScheme::PGPMime,
            sign: false,
        }
    }
}

/// Recipient public address keys retrived from the `keys/all` route.
mod recipient_keys {
    use proton_crypto_account::keys::APIUnverifiedPublicAddressKeyGroup;

    use super::*;
    pub fn load_recipient_public_key_internal<Provider: PGPProviderSync>(
        provider: &Provider,
    ) -> PublicAddressKeys<Provider::PublicKey> {
        let address_keys = vec![APIPublicKey {
            source: APIPublicKeySource::Proton,
            flags: KeyFlag::from(3_u32),
            primary: true,
            public_key: "-----BEGIN PGP PUBLIC KEY BLOCK-----\nVersion: ProtonMail\n\nxjMEZsNYwBYJKwYBBAHaRw8BAQdAfgCr5GL1OaFneBvtkCLKtXfxXhBuOu7w\nErbqLyzOqO3NKWx1YnVxYUBwcm90b24uYmxhY2sgPGx1YnVxYUBwcm90b24u\nYmxhY2s+wowEEBYKAD4FgmbDWMAECwkHCAmQjHhrAnsKfCUDFQgKBBYAAgEC\nGQECmwMCHgEWIQSAuZravmHpbXRi/mKMeGsCewp8JQAA+1UA/2q1hjPcwVC9\njkUPV7BJkwJNTBqSgbWjgaIzBbWb6FM/APwPAwVTmYwFqEucYSPtphEPgWvS\nySWdK6bxJGhBsMWFBsLAHgQQFggAkAWCZsNY/gWDAO1OAAmQ1NIaBVGnsuw1\nFAAAAAAAHAAQc2FsdEBub3RhdGlvbnMub3BlbnBncGpzLm9yZ+WIj1ihxJT6\n1ZQV5FYf3DksHFRlc3QgT3BlblBHUCBDQSA8dGVzdC1vcGVucGdwLWNhQHBy\nb3Rvbi5tZT4WIQQ2FUO/DaVtpgyz2pTU0hoFUaey7AAAeUoA/0RZGi8Zi67e\nJ3s2oaXj3ojVEiZNPsIIRkKBlIqXZjseAQC6QUF0HCzOWrJMpqKDi5l4OJ4N\nubnqCt218efp5/UaD844BGbDWMASCisGAQQBl1UBBQEBB0BBom9x/zFNXn6o\nQzcGzSuwgciKUrpbb4qeuz/7KnyTNgMBCAfCeAQYFgoAKgWCZsNYwAmQjHhr\nAnsKfCUCmwwWIQSAuZravmHpbXRi/mKMeGsCewp8JQAAssoA+wTZTc5+zOlQ\nsvJae7cLzw5JzT9taAvLS7WTw6YNqd7zAP42f8YSAse1yDHrGHJlUpzeWYkP\nj27RcNERSwJBd6hxCg==\n=/8F+\n-----END PGP PUBLIC KEY BLOCK-----\n".to_owned(),
        }];

        let skl = SignedKeyList {
            min_epoch_id: Some(175),
            max_epoch_id: Some(178),
            expected_min_epoch_id: None,
            data: Some(SKLDataJson::from(
                r#"[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"80b99adabe61e96d7462fe628c786b027b0a7c25\",\"SHA256Fingerprints\":[\"65cf7d74c8e50fb0b864c1a5ea8c49b31e3ecd7bfd0c76363860851b81c6c1dc\",\"6f9071e34e2076a72e111ed123cbf77ddbbc749af8dca41fc8a30489c1fa04ae\"]}]"#,
            )),
            obsolescence_token: None,
            revision: 1,
            signature: Some(SKLSignature::from(
                "-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwqkEARYKAFsFgmbDWPwJkIx4awJ7CnwlMxSAAAAAABEAGWNvbnRleHRAcHJv\ndG9uLmNoa2V5LXRyYW5zcGFyZW5jeS5rZXktbGlzdBYhBIC5mtq+YeltdGL+\nYox4awJ7CnwlAACw9gD/dfcPJFW1rBhRWr3geEU2v9955hhAqXmy1JJrOeYV\noMcA/jsXeIa+F/ovtKbtilQl965obBISN409xZiRSDzDieMF\n=bnsy\n-----END PGP SIGNATURE-----\n",
            )),
        };

        let address_key_keygroup = APIPublicAddressKeyGroup {
            keys: address_keys,
            signed_key_list: Some(skl),
        };

        let api_keys = APIPublicAddressKeys {
            address_keys: address_key_keygroup,
            catch_all_keys: None,
            unverified_keys: None,
            warnings: vec![],
            proton_mx: true,
            is_proton: false,
        };
        api_keys.import(provider).unwrap()
    }

    pub fn load_recipient_public_key_internal_v6<Provider: PGPProviderSync>(
        provider: &Provider,
    ) -> PublicAddressKeys<Provider::PublicKey> {
        let address_keys = vec![APIPublicKey {
            source: APIPublicKeySource::Proton,
            flags: KeyFlag::from(3_u32),
            primary: true,
            public_key: "-----BEGIN PGP PUBLIC KEY BLOCK-----\nVersion: ProtonMail\n\nxjMEZ1Gt1BYJKwYBBAHaRw8BAQdAxKiYR++GZGOrNQApcd4T5IsQOahcgqf6\n7mfHai+BaHXNPXBxYy1hY2NvdW50LWtleXNAcHJvdG9uLmJsYWNrIDxwcWMt\nYWNjb3VudC1rZXlzQHByb3Rvbi5ibGFjaz7CwBEEExYKAIMFgmdRrdQDCwkH\nCZBMEsE4/XND4EUUAAAAAAAcACBzYWx0QG5vdGF0aW9ucy5vcGVucGdwanMu\nb3JnceYp+VpXsE/UFEZX06Snp/NA0s+wSTDBGFqWvodvSvoDFQoIBBYAAgEC\nGQECmwMCHgEWIQQVMa0O7aKLGj3ucD9MEsE4/XND4AAAdJIBAKhHoebqye62\nYzfgVKX86G/ENOMVi9L2ckBt7uOo3DKHAQCKAM3Jp7OwUtdPsRI1b/EyGDJw\nfieuWyPIAsYssjtDCcLAHgQQFggAkAWCZ1GuIAWDAO1OAAmQ1NIaBVGnsuw1\nFAAAAAAAHAAQc2FsdEBub3RhdGlvbnMub3BlbnBncGpzLm9yZznFEvC6iWyk\nf6w44W4tiDssHFRlc3QgT3BlblBHUCBDQSA8dGVzdC1vcGVucGdwLWNhQHBy\nb3Rvbi5tZT4WIQQ2FUO/DaVtpgyz2pTU0hoFUaey7AAAG4ABAIB9hYnJ2aCO\ncNtLBq7O2b/8bLzcD29JiV4J83sDrbq7AQCz9kKkL6Hm/Oh0SQRQ0oRaYEfK\nBt6Nd8Y7orU5hoBLAM44BGdRrdQSCisGAQQBl1UBBQEBB0DeKNu9ZFv4bMy0\nOVNcWy2W4XOrBHuVC7rEgka+njfhQgMBCAfCvgQYFgoAcAWCZ1Gt1AmQTBLB\nOP1zQ+BFFAAAAAAAHAAgc2FsdEBub3RhdGlvbnMub3BlbnBncGpzLm9yZ4Rb\ntzjVe2M7c+75bsOB0xeeQoLhVhGjkrigIbSeoJpdApsMFiEEFTGtDu2iixo9\n7nA/TBLBOP1zQ+AAADqMAQCW1kCU4xCEv513iA5DcdhnpboyrjRgZiwp+jQH\nBXXIaAEAp26qhBe4FJYrM0tyXzj70vAE//4GSc/09vNw8YYM9ww=\n=X6z+\n-----END PGP PUBLIC KEY BLOCK-----\n".to_owned(),
        }, APIPublicKey {
            source: APIPublicKeySource::Proton,
            flags: KeyFlag::from(3_u32),
            primary: true,
            public_key: "-----BEGIN PGP PUBLIC KEY BLOCK-----\nVersion: ProtonMail\n\nxioGZ1HqGRsAAAAgsLRL1RXfGij8JJAsHHhYE78buH4smu2/7Ht9X6NXnRbC\nrQYfGwoAAAA+BYJnUeoZAwsJBwUVCggODAQWAAIBApsDAh4BIqEGQfSMr87C\nfo1hpb1YDIBctaqhdUG7hC/EP7FYweEjoRMAAAAACswgYwlj2tOb00Ll68EV\nziLV1tsRxWcs7Y5ADDzDh5mBa8CysCwB4yGF///HG6W5kiddbY9fyJe9lCVw\nJ43aPR2t0rF7J2t0h/roVFjLv6Mf9BkIfYMw8+kIlGLas8nwR4MHzT1wcWMt\nYWNjb3VudC1rZXlzQHByb3Rvbi5ibGFjayA8cHFjLWFjY291bnQta2V5c0Bw\ncm90b24uYmxhY2s+wpsGExsKAAAALAWCZ1HqGQIZASKhBkH0jK/Own6NYaW9\nWAyAXLWqoXVBu4QvxD+xWMHhI6ETAAAAAFH1IHTT7MrfLOtw9IUmIeTMoSDk\ngIC5OiLanClndgSK0ZhXvLi7JoE4bbR8jCIxULEZ1t8VV1YdS3nYecgqWnBk\nBseah/zlrTfRM974xB6CG34c1U33NMXEN4TGeylNpt+4BcLAHgQQFggAkAWC\nZ1HqZAWDAO1OAAmQ1NIaBVGnsuw1FAAAAAAAHAAQc2FsdEBub3RhdGlvbnMu\nb3BlbnBncGpzLm9yZzdm9OlYOIUI84+RcGeDiLosHFRlc3QgT3BlblBHUCBD\nQSA8dGVzdC1vcGVucGdwLWNhQHByb3Rvbi5tZT4WIQQ2FUO/DaVtpgyz2pTU\n0hoFUaey7AAAZCUBAKs29RLbxsPOZyQqWC6F9kXN1LS9W6De4buqKn/F0Wm/\nAP4lXIRE0QW5n5QugNBhl2l0Vp9m37/H7CtG3H0PCVidB84qBmdR6hkZAAAA\nIL2N9waOtHEZoIjYWX96uixIWnKzax7iYFG9GVxl/4B1wpsGGBsKAAAALAWC\nZ1HqGQKbDCKhBkH0jK/Own6NYaW9WAyAXLWqoXVBu4QvxD+xWMHhI6ETAAAA\nAB1kIKUuISpNAl4k2i77yQEztf2++kYkwllY13VjCwXgx88WjpZoAriNZRQ7\noUkE/+KejIj9YwzxQbjPw4SldPCOSf585O/SA1MyOEAkIJUqmPsFMMYWWzAv\nTMm3qlbMufPWAQ==\n=osVR\n-----END PGP PUBLIC KEY BLOCK-----\n".to_owned(),
        },
        ];

        let skl = SignedKeyList {
            min_epoch_id: Some(19),
            max_epoch_id: Some(31),
            expected_min_epoch_id: None,
            data: Some(SKLDataJson::from(
                r#"[{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"1531ad0eeda28b1a3dee703f4c12c138fd7343e0\",\"SHA256Fingerprints\":[\"d57141c407d73870968842ca4898398897f49999c740495862f0039ffd978d7c\",\"2dcbe9648eaa7bec07bdb466c8c9934469851c808c26e80b4bf03476a7e09d51\"]},{\"Primary\":1,\"Flags\":3,\"Fingerprint\":\"41f48cafcec27e8d61a5bd580c805cb5aaa17541bb842fc43fb158c1e123a113\",\"SHA256Fingerprints\":[\"41f48cafcec27e8d61a5bd580c805cb5aaa17541bb842fc43fb158c1e123a113\",\"ba6610eaeb34f2fbd8e9aadf6df59202adb9a8627bd901d3200a3ab2208f77b1\"]}]"#,
            )),
            obsolescence_token: None,
            revision: 7,
            signature: Some(SKLSignature::from(
                "-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\n\nwsAvBAEWCgChBYJnUepmCZBMEsE4/XND4DMUgAAAAAARABljb250ZXh0QHBy\nb3Rvbi5jaGtleS10cmFuc3BhcmVuY3kua2V5LWxpc3RFFAAAAAAAHAAgc2Fs\ndEBub3RhdGlvbnMub3BlbnBncGpzLm9yZzCfASjbrFGVAE7c6Jzb28nrmc0K\n+s8OiBHrDnoAnEztFiEEFTGtDu2iixo97nA/TBLBOP1zQ+AAAGEHAQCjxEzK\n3oeMYrvzejqZW0LTZf5Qz+rKgGjKG/Ep3BUjEwEAxdRX20z9SwDv3tB2JEDW\neH8zAvwMfSMkI6J3M5xDyQHCwAwGARsKAAAAXQWCZ1HqZjMUgAAAAAARABlj\nb250ZXh0QHByb3Rvbi5jaGtleS10cmFuc3BhcmVuY3kua2V5LWxpc3QioQZB\n9IyvzsJ+jWGlvVgMgFy1qqF1QbuEL8Q/sVjB4SOhEwAAAADa5CCfgtMLYqz3\n2WSbiW5nu6Hlzcl+xK4vL4ssUl4mQ9UsDmioI6HHyJ1Pe1aPq1dw6lNjBOJn\nltYkIw4ePM7SXZ7UDzxOPXGqk0VA2gLbgLh8NHDrNEjmMzHy2MX7cR7ePwU=\n=Pfkv\n-----END PGP SIGNATURE-----\n",
            )),
        };

        let address_key_keygroup = APIPublicAddressKeyGroup {
            keys: address_keys,
            signed_key_list: Some(skl),
        };

        let api_keys = APIPublicAddressKeys {
            address_keys: address_key_keygroup,
            catch_all_keys: None,
            unverified_keys: None,
            warnings: vec![],
            proton_mx: true,
            is_proton: false,
        };
        api_keys.import(provider).unwrap()
    }

    pub fn load_recipient_public_key_external<Provider: PGPProviderSync>(
        provider: &Provider,
    ) -> PublicAddressKeys<Provider::PublicKey> {
        let unverified_keygroup = APIUnverifiedPublicAddressKeyGroup {
            keys: vec![APIPublicKey {
                source: APIPublicKeySource::WKD,
                flags: KeyFlag::from(3_u32),
                primary: true,
                public_key: "-----BEGIN PGP PUBLIC KEY BLOCK-----\nVersion: ProtonMail\n\nxjMEZf15lRYJKwYBBAHaRw8BAQdArPz06hKiOUYSVs6dbHpKSh63bW5/QyIF\nqRvJ5wOALJnNMkx1a2FzIEJ1cmtoYWx0ZXIgPGtleXRyYW5zcGFyZW5jeW1h\naWxlckBnbWFpbC5jb20+wo8EExYIADcWIQSNEf53FU6EMmZs43pG8PpwjTNi\nIAUCZf15lQUJBaOagAIbAwQLCQgHBRUICQoLBRYCAwEAAAoJEEbw+nCNM2Ig\naX0BANKGrENgM7nbpt5uORfaT5JLx695q1RgKDetm6bQhB1/AQDHvY3oha+e\nabN+yKcOWKlvvNpbbbYzjunnrmfm7d+HDM44BGX9eZUSCisGAQQBl1UBBQEB\nB0Aq4KRFu4d/XmR2UEGjsXeWCWvvKUkzsCR/wRDn8E/lRQMBCAfCfgQYFggA\nJhYhBI0R/ncVToQyZmzjekbw+nCNM2IgBQJl/XmVBQkFo5qAAhsMAAoJEEbw\n+nCNM2IgEzcBAPqEmyOcnbzbsGJaZ5uFEA3OfGH7anEg2xEbfZ0jxAh0AP9n\nsO+JqQrVW5m3aGW4MRMFRjnC2DIHthThNQMw1bZpDQ==\n=ziuc\n-----END PGP PUBLIC KEY BLOCK-----\n".to_owned(),
            }]
        };

        // Populate APIPublicAddressKeys struct
        let api_keys = APIPublicAddressKeys {
            address_keys: APIPublicAddressKeyGroup::default(),
            catch_all_keys: Some(APIPublicAddressKeyGroup::default()),
            unverified_keys: Some(unverified_keygroup),
            warnings: vec![], // Warnings is an empty list
            proton_mx: false,
            is_proton: false,
        };
        api_keys.import(provider).unwrap()
    }

    pub fn load_recipient_public_key_external_no_keys<Provider: PGPProviderSync>(
        provider: &Provider,
    ) -> PublicAddressKeys<Provider::PublicKey> {
        // Populate APIPublicAddressKeys struct
        let api_keys = APIPublicAddressKeys {
            address_keys: APIPublicAddressKeyGroup::default(),
            catch_all_keys: Some(APIPublicAddressKeyGroup::default()),
            unverified_keys: None,
            warnings: vec![], // Warnings is an empty list
            proton_mx: false,
            is_proton: false,
        };
        api_keys.import(provider).unwrap()
    }
}

/// Unlocked sender address keys.
mod sender_keys {
    use proton_crypto_account::keys::UnlockedAddressKeys;

    use super::*;

    pub fn load_sender_address_keys<Provider: PGPProviderSync>(
        provider: &Provider,
    ) -> UnlockedAddressKeys<Provider> {
        common::create_account_unlocked_address_keys(provider, ADDRESS_KEY, "password")
    }
}

mod send_logic {
    use std::str::FromStr;

    use super::*;
    use proton_crypto_account::keys::UnlockedAddressKeys;
    use send_request::PackageSignaturesMode;

    pub fn setup_test_environment<P>(
        pgp: &P,
        draft_loader: fn() -> models::DraftMessage,
    ) -> (
        CryptoMailSettings,
        ComposerPreference,
        models::DraftMessage,
        UnlockedAddressKeys<P>,
    )
    where
        P: PGPProviderSync,
    {
        let mail_settings = models::load_mail_settings();
        let draft = draft_loader();
        let sender_keys = sender_keys::load_sender_address_keys(pgp);

        let composer_preferences =
            ComposerPreference::new(EmailMimeType::from_str(&draft.mime_type).unwrap());

        (mail_settings, composer_preferences, draft, sender_keys)
    }

    pub fn create_send_preferences<P>(
        pgp: &P,
        mail_settings: CryptoMailSettings,
        composer_preferences: ComposerPreference,
        keys_loader: fn(&P) -> PublicAddressKeys<P::PublicKey>,
        expected_mime_type: PackageMimeType,
    ) -> SendPreferences<P::PublicKey>
    where
        P: PGPProviderSync,
    {
        let recipient_keys = keys_loader(pgp);
        let recipient_preferences = SendPreferences::new(
            recipient_keys,
            None,
            UnixTimestamp::new(1_734_001_426),
            &mail_settings,
            composer_preferences,
        )
        .unwrap();

        assert_eq!(recipient_preferences.mime_type, expected_mime_type);
        recipient_preferences
    }

    pub fn process_package<P>(
        pgp: &P,
        recipient_email: &str,
        attachments: &[models::Attachment],
        encrypted_body: &EncryptedPackageBody,
        sender_keys: &[UnlockedAddressKey<P>],
        recipient_preferences: SendPreferences<P::PublicKey>,
    ) -> send_request::SendPackage
    where
        P: PGPProviderSync,
    {
        let mut package = send_request::SendPackage {
            body: Some(encrypted_body.encrypted_body.clone().into()),
            mime_type: encrypted_body.mime_type,
            ..Default::default()
        };

        process_recipient(
            recipient_email,
            &mut package,
            pgp,
            &encrypted_body.session_key,
            attachments,
            sender_keys,
            recipient_preferences,
        );

        package.package_type = package.addresses.iter().fold(0, |acc, (_, address)| {
            acc | i32::from(address.address_type.type_value())
        });

        package
    }

    fn process_recipient<P>(
        recipient_mail: &str,
        top_package: &mut send_request::SendPackage,
        pgp: &P,
        draft_sk: &InboxSessionKey,
        attachments: &[models::Attachment],
        sender_keys: &[impl AsRef<P::PrivateKey>],
        recipient_send_preferences: SendPreferences<P::PublicKey>,
    ) where
        P: PGPProviderSync,
    {
        let mut address_package = send_request::SendAddress {
            address_type: recipient_send_preferences.pgp_scheme,
            ..Default::default()
        };

        match recipient_send_preferences.pgp_scheme {
            PackageCryptoType::ProtonMail | PackageCryptoType::PgpMime => {
                let recipient_key = recipient_send_preferences
                    .selected_key
                    .expect("No recipient key");

                let recipient_key_packet = draft_sk
                    .encrypt_to_recipient(pgp, &recipient_key)
                    .expect("Should be able to create recipient key packet");

                address_package.body_key_packet = Some(recipient_key_packet);

                if recipient_send_preferences.pgp_scheme == PackageCryptoType::ProtonMail {
                    process_attachments(
                        &mut address_package,
                        pgp,
                        attachments,
                        sender_keys,
                        &recipient_key,
                        recipient_send_preferences.sign,
                    );
                }
            }

            PackageCryptoType::Cleartext => {
                top_package.body_key = Some(draft_sk.to_owned().into());
                process_attachment_cleartext(top_package, pgp, attachments, sender_keys);
                address_package.signature = Some(PackageSignaturesMode::None);
            }

            PackageCryptoType::ClearMime => {
                top_package.body_key = Some(draft_sk.to_owned().into());
                address_package.signature =
                    Some(PackageSignaturesMode::from(recipient_send_preferences.sign));
            }

            PackageCryptoType::PgpInline | PackageCryptoType::EncryptedOutside => {
                panic!("Not supported")
            }
        }

        top_package
            .addresses
            .insert(recipient_mail.to_owned(), address_package);
    }

    fn process_attachment_cleartext<P>(
        top_package: &mut send_request::SendPackage,
        pgp: &P,
        attachments: &[models::Attachment],
        sender_keys: &[impl AsRef<P::PrivateKey>],
    ) where
        P: PGPProviderSync,
    {
        for attachment in attachments {
            let attachment_info = attachment
                .decrypt_attachment_info(pgp, sender_keys)
                .expect("Failed to extract attachment info");

            top_package
                .attachment_keys
                .insert(attachment.id.clone(), attachment_info.session_key.into());
        }
    }

    fn process_attachments<P>(
        address_package: &mut send_request::SendAddress,
        pgp: &P,
        attachments: &[models::Attachment],
        sender_keys: &[impl AsRef<P::PrivateKey>],
        recipient_key: &P::PublicKey,
        sign: bool,
    ) where
        P: PGPProviderSync,
    {
        let mut attachment_key_packets = HashMap::with_capacity(attachments.len());
        let mut attachment_enc_signatures = HashMap::with_capacity(attachments.len());
        let mut sign = sign;

        for attachment in attachments {
            if attachment.signature.is_none() && attachment.enc_signature.is_none() {
                sign = false;
            }

            // Decrypt attachment information using sender's keys
            let attachment_info = attachment
                .decrypt_attachment_info(pgp, sender_keys)
                .expect("Failed to extract attachment info");

            // Encrypt the attachment session key to the recipient
            let recipient_attachment_kp = attachment_info
                .encrypt_session_key_to_recipient(pgp, recipient_key)
                .expect("Failed to encrypt session key to recipient");

            // Optionally encrypt the signature to the recipient
            if let Some(enc_signature) = attachment_info
                .encrypt_signature_to_recipient(pgp, recipient_key)
                .expect("Failed to encrypt signature to recipient")
            {
                attachment_enc_signatures
                    .insert(attachment.id.clone(), enc_signature.encode_base64());
            }

            attachment_key_packets.insert(attachment.id.clone(), recipient_attachment_kp);
        }

        address_package.attachment_key_packets = Some(attachment_key_packets);
        address_package.attachment_enc_signatures = Some(attachment_enc_signatures);
        address_package.signature = Some(PackageSignaturesMode::from(sign));
    }

    #[derive(Debug)]
    pub struct MimeBody(pub Vec<u8>);

    impl EncryptablePackage for MimeBody {
        fn package_mime_type(&self) -> PackageMimeType {
            PackageMimeType::Multipart
        }

        fn package_body_content(&self) -> &[u8] {
            &self.0
        }
    }

    pub fn build_mime<P>(
        draft: &models::DraftMessage,
        pgp: &P,
        sender_keys: &[impl AsRef<P::PrivateKey>],
    ) -> MimeBody
    where
        P: PGPProviderSync,
    {
        let mut content = Vec::new();
        let (body, _) = draft.decrypt(pgp, sender_keys).unwrap();

        let DecryptedBody::Plain(plain_body) = body else {
            panic!("Mime body found in draft");
        };

        let mut builder = InboxMimeBuilder::new()
            .text_body("Test")
            .html_body(&plain_body);

        for attachment in &draft.attachments {
            match attachment.disposition {
                proton_crypto_inbox_mime::Disposition::Attachment => {
                    builder = builder.attachment(
                        &attachment.name,
                        Some(&attachment.mime_type),
                        b"loaded content".to_vec(),
                    );
                }

                proton_crypto_inbox_mime::Disposition::Inline => {
                    if let Some(content_id) = attachment.headers.get("content-id") {
                        builder = builder.inline_attachment(
                            content_id,
                            &attachment.name,
                            Some(&attachment.mime_type),
                            b"loaded content".to_vec(),
                        );
                    } else {
                        builder = builder.attachment(
                            &attachment.name,
                            Some(&attachment.mime_type),
                            b"loaded content".to_vec(),
                        );
                    }
                }
            }
        }

        builder
            .write_to(&mut content)
            .expect("Failed to write mime");

        MimeBody(content)
    }
}

/// Validates that the recipient can decrypt the sent message.
fn validate_decryption(
    package: &send_request::SendPackage,
    validation_key_decryption: &str,
    expected_message: &str,
) {
    let pgp = new_pgp_provider();
    let sender_keys = sender_keys::load_sender_address_keys(&pgp);

    let validation_key = pgp
        .private_key_import(validation_key_decryption, "password", DataEncoding::Armor)
        .unwrap();

    if let Some(exposed_body_key) = &package.body_key {
        let imported_key = pgp
            .session_key_import(
                exposed_body_key.key.decode().unwrap(),
                exposed_body_key.algorithm,
            )
            .unwrap();

        let result = pgp
            .new_decryptor()
            .with_session_key(imported_key)
            .with_verification_key_refs(&sender_keys)
            .decrypt(package.body.as_ref().unwrap(), DataEncoding::Bytes)
            .expect("expect decryption to succeed");

        result
            .verification_result()
            .expect("Signature verification failed");

        match package.mime_type {
            PackageMimeType::Multipart => {
                let (message, _) = MimeProcessor::process_mime("message_id", result.as_bytes())
                    .expect("Failed to process mime messsage");
                assert_eq!(&message.body, expected_message);
            }
            _ => {
                assert_eq!(
                    to_sanitized_string(result.as_bytes()).unwrap(),
                    expected_message
                );
            }
        }

        return;
    }

    for address in package.addresses.values() {
        let mut pgp_message_bin = address
            .body_key_packet
            .as_ref()
            .map(|packet| packet.decode().unwrap())
            .expect("No key packet found");

        pgp_message_bin.extend_from_slice(package.body.as_ref().unwrap());

        let pgp_message = pgp.armorer().armor_message(pgp_message_bin).unwrap();

        let mime_type = match package.mime_type {
            PackageMimeType::Multipart => "multipart/mixed",
            _ => "text/html",
        };

        let message = models::DraftMessage {
            id: "id".to_owned(),
            subject: "subject".to_owned(),
            sender: "sender".to_owned(),
            to_list: Vec::default(),
            num_attachments: 0,
            body: String::from_utf8(pgp_message).unwrap(),
            mime_type: mime_type.to_owned(),
            attachments: Vec::default(),
        };

        let (body, verifier) = message
            .decrypt(&pgp, &[validation_key.clone()])
            .expect("decryption should succeed");

        verifier
            .verify_signature(&pgp, &sender_keys)
            .expect("Signature verification failed");

        match body {
            DecryptedBody::Plain(plain) => assert_eq!(&plain, expected_message),
            DecryptedBody::Mime(processed_message) => {
                assert_eq!(&processed_message.body, expected_message);
            }
        }
    }
}
