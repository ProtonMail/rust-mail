//! [Confluence Sending Preferences](https://confluence.protontech.ch/display/MAILFE/Send+preferences+for+outgoing+email)
//! [Advanced encryption setting](https://confluence.protontech.ch/display/MAILFE/Advanced+PGP+settings)
use proton_crypto_account::{
    keys::{
        DecryptedAddressKey, InboxPublicKeys, KeyTrust, PGPScheme, PinnedPublicKeys, RecipientType,
    },
    proton_crypto::{
        crypto::{PrivateKey, PublicKey, UnixTimestamp},
        keytransparency::KTVerificationResult,
    },
};

use crate::message::packages::PackageMimeType;

use super::{CryptoPackageTypeError, EncryptionPreferencesError, UserWarning};

/// A helper type that contains the default PGP preferences
/// extracted from the user's mailsettings.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct CryptoMailSettings {
    /// The default PGP scheme to use.
    pub pgp_scheme: PGPScheme,

    /// The default content mime type to use.
    pub mime_type: PackageMimeType,

    /// If mails should be signed by default.
    pub sign: bool,
}

/// All possible encryption types as requested by the Proton API.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum CryptoPackageType {
    /// Encrypted using `ProtonMail`'s native encryption.
    ProtonMail,

    /// Encrypted for users outside of `ProtonMail`'s system.
    EncryptedOutside,

    /// Message is not encrypted and is in plain text.
    Cleartext,

    /// PGP encryption using inline PGP format.
    PgpInline,

    /// PGP encryption using MIME format.
    PgpMime,

    /// Cleartext message with MIME formatting.
    ClearMime,
}

impl CryptoPackageType {
    pub fn type_value(&self) -> i32 {
        match self {
            CryptoPackageType::ProtonMail => 1,
            CryptoPackageType::EncryptedOutside => 2,
            CryptoPackageType::Cleartext => 4,
            CryptoPackageType::PgpInline => 8,
            CryptoPackageType::PgpMime => 16,
            CryptoPackageType::ClearMime => 32,
        }
    }

    pub fn enum_of(value: i32) -> Option<CryptoPackageType> {
        match value {
            1 => Some(CryptoPackageType::ProtonMail),
            2 => Some(CryptoPackageType::EncryptedOutside),
            4 => Some(CryptoPackageType::Cleartext),
            8 => Some(CryptoPackageType::PgpInline),
            16 => Some(CryptoPackageType::PgpMime),
            32 => Some(CryptoPackageType::ClearMime),
            _ => None,
        }
    }

    pub fn from_scheme(scheme: PGPScheme, encrypt: bool, sign: bool) -> CryptoPackageType {
        match scheme {
            PGPScheme::PGPMime => {
                if !encrypt && sign {
                    CryptoPackageType::ClearMime
                } else if encrypt {
                    CryptoPackageType::PgpMime
                } else {
                    CryptoPackageType::Cleartext
                }
            }
            PGPScheme::PGPInline => {
                if encrypt {
                    CryptoPackageType::PgpInline
                } else {
                    CryptoPackageType::Cleartext
                }
            }
        }
    }
}

impl TryFrom<i32> for CryptoPackageType {
    type Error = CryptoPackageTypeError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::enum_of(value).ok_or(CryptoPackageTypeError::Parse(value))
    }
}

/// A type that stores public keys and preferences for encrypting data.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct InboxSendPreferences<Pub: PublicKey> {
    /// Indicates whether the data should be encrypted.
    pub encrypt: bool,

    /// Indicates whether the data should be signed.
    pub sign: bool,

    /// Specifies the PGP (Pretty Good Privacy) encryption scheme to be used.
    pub pgp_scheme: CryptoPackageType,

    /// Defines the MIME type of the package.
    pub mime_type: PackageMimeType,

    /// Optionally stores the selected public key for encryption.
    pub selected_key: Option<Pub>,

    /// Indicates whether the selected key is pinned (persistently selected).
    pub is_selected_key_pinned: bool,

    /// Indicates whether the recipient has an Proton API key.
    pub has_api_keys: bool,

    /// Stores information if the user should be warned about te key selection.
    pub user_warning: Option<UserWarning>,

    /// Result of the key transparency verification process.
    pub key_transparency_verification: KTVerificationResult,
}

impl<Pub: PublicKey> InboxSendPreferences<Pub> {
    /// Creates an [`InboxSendPreferences`] instance using unlocked address keys.
    ///
    /// This function determines the appropriate PGP encryption preferences for sending to and address of the same user account.
    /// The first key in the address key list is selected as the primary key for sending,
    /// which is then validated to ensure it is neither obsolete nor compromised.
    ///
    /// # Parameters
    ///
    /// - `address_keys`   - A reference to a slice of `DecryptedAddressKey` containing the recipient's decrypted address keys.
    ///                      The first key is used as the primary key for setting the preferences.
    /// - `mail_settings`  - A reference to the `CryptoMailSettings` which defines the default encryption, signing, and MIME type settings.
    ///
    /// # Errors
    ///
    /// - `EncryptionPreferencesError::InvalidPrimaryKey` - If the primary key is obsolete or compromised.
    /// - `EncryptionPreferencesError::NoKeyFound` - If no address keys are provided.
    pub fn create_from_unlocked_address_keys<Priv: PrivateKey>(
        address_keys: &[DecryptedAddressKey<Priv, Pub>],
        mail_settings: &CryptoMailSettings,
    ) -> Result<Self, EncryptionPreferencesError> {
        let Some(primary_address_key) = address_keys.first() else {
            return Err(EncryptionPreferencesError::NoKeyFound);
        };

        if primary_address_key.flags.is_obsolete() || primary_address_key.flags.is_compromised() {
            return Err(EncryptionPreferencesError::InvalidPrimaryKey(
                primary_address_key.flags.is_obsolete(),
                primary_address_key.flags.is_compromised(),
            ));
        }
        Ok(InboxSendPreferences {
            encrypt: true,
            sign: true,
            pgp_scheme: CryptoPackageType::ProtonMail,
            mime_type: mail_settings.mime_type,
            is_selected_key_pinned: false,
            has_api_keys: true,
            user_warning: None,
            selected_key: Some(primary_address_key.public_key.to_owned()),
            key_transparency_verification: KTVerificationResult::Ok(()),
        })
    }

    /// Selects PGP emails sending preferences and the encryption key by creating a [`InboxSendPreferences`] instance.
    ///
    /// This function determines the appropriate encryption and signing preferences for the recipient
    /// by analyzing the provided public keys (`api_keys`), optional pinned public keys (`vcard_keys`),
    /// and the user's mail settings. The function checks for the validity of keys, selects the best
    /// key for encryption, and selects relevant settings for MIME type, encryption, signing, and PGP scheme,
    /// which are used to create the packages for sending emails.
    ///
    /// # Parameters
    ///
    /// - `api_keys`         - A reference to the `InboxPublicKeys` containing the recipient's public keys.
    /// - `vcard_keys`       - An optional reference to `PinnedPublicKeys` for additional encryption key preferences extracted from the v-card.
    /// - `mail_settings`    - A reference to the `CryptoMailSettings` defining the user's default encryption and signing settings (Should come from mail settings).
    /// - `encryption_time`  - The `UnixTimestamp` representing the current encryption time for validating key expiration and revocation.
    ///
    /// # Errors
    ///
    /// - `EncryptionPreferencesError::InvalidPrimaryKey` - If the primary key is obsolete or compromised.
    /// - `EncryptionPreferencesError::InternalUserWithNoKeys` - If an internal user has no available keys.
    /// - `EncryptionPreferencesError::NoKeyFound` - If no valid key can be found for an external recipient with enabled encryption.
    pub fn create_from_public_address_keys(
        api_keys: &InboxPublicKeys<Pub>,
        vcard_keys: Option<&PinnedPublicKeys<Pub>>,
        mail_settings: &CryptoMailSettings,
        encryption_time: UnixTimestamp,
    ) -> Result<Self, EncryptionPreferencesError> {
        // The first key returned from the API is the primary key that should be used for encryption.
        // If there is one it must be valid.
        if let Some(key) = api_keys.public_keys.first() {
            if key.flags.is_obsolete() || key.flags.is_compromised() {
                return Err(EncryptionPreferencesError::InvalidPrimaryKey(
                    key.flags.is_obsolete(),
                    key.flags.is_compromised(),
                ));
            }
        }

        let is_internal = api_keys.recipient_type == RecipientType::Internal;
        let has_api_keys = !api_keys.public_keys.is_empty();
        if is_internal && !has_api_keys {
            // Proton users should always have keys.
            return Err(EncryptionPreferencesError::InternalUserWithNoKeys);
        }

        // Extract the preferences from the vcard key data or fallback to mail settings data.
        let (
            mut send_settings_mime_type,
            mut send_settings_encrypt,
            mut send_settings_sign,
            mut send_settings_pgp_type,
        ) = Self::extract_send_settings(vcard_keys, mail_settings, is_internal);

        // Select the pinned key.
        let selected_encryption_key = vcard_keys
            .as_ref()
            .and_then(|pinned_keys| pinned_keys.select_encryption_key(api_keys, encryption_time));
        let (selected_pinned_key, user_warning, is_selected_key_pinned) =
            match selected_encryption_key {
                Some(KeyTrust::Trusted(key)) => (Some(key), None, true),
                Some(KeyTrust::PromptUserToTrust(key)) => {
                    let fingerprint = key.key_fingerprint();
                    (
                        Some(key),
                        Some(UserWarning::PromptUserToTrust(fingerprint)),
                        false,
                    )
                }
                None => {
                    if matches!(vcard_keys, Some(keys) if !keys.pinned_keys.is_empty()) {
                        (None, Some(UserWarning::NoValidPinnedKey), false)
                    } else {
                        (None, None, false)
                    }
                }
            };
        // Select the api key: the first key is the primary key for encryption.
        let selected_api_key = api_keys
            .public_keys
            .first()
            .map(|key| key.public_keys.clone());

        // Select keys and modify flags if necessary
        let selected_key = match api_keys.recipient_type {
            RecipientType::Internal => {
                // Internal messages are always encrypted or signed.
                send_settings_encrypt = true;
                send_settings_sign = true;
                selected_pinned_key.or(selected_api_key)
            }
            RecipientType::External => {
                if vcard_keys.is_some()
                    && send_settings_encrypt
                    && selected_api_key.is_none()
                    && selected_pinned_key.is_none()
                {
                    // No valid key can be found for an external recipient with enabled encryption.
                    return Err(EncryptionPreferencesError::NoKeyFound);
                }

                if api_keys.is_internal_with_disabled_e2ee {
                    send_settings_encrypt = false;
                    send_settings_sign = false;
                    send_settings_pgp_type = CryptoPackageType::Cleartext;
                    send_settings_mime_type = mail_settings.mime_type;
                };

                match (selected_api_key, selected_pinned_key) {
                    (None, None) => {
                        if vcard_keys.is_none() {
                            // No v-card information and no keys.
                            send_settings_encrypt = false;
                            send_settings_pgp_type = CryptoPackageType::Cleartext;
                            send_settings_mime_type = mail_settings.mime_type;
                        }
                        None
                    }
                    (Some(api_key), None) => Some(api_key),
                    (_, Some(pinned_key)) => Some(pinned_key),
                }
            }
        };

        Ok(InboxSendPreferences {
            encrypt: send_settings_encrypt,
            sign: send_settings_encrypt || send_settings_sign,
            pgp_scheme: send_settings_pgp_type,
            mime_type: send_settings_mime_type,
            selected_key,
            is_selected_key_pinned,
            user_warning,
            has_api_keys,
            key_transparency_verification: api_keys.key_transparency_verification.clone(),
        })
    }

    /// Helper function to determine the PGP sending settings based on the vcard of the recipient
    /// and the data from the mail settings.
    fn extract_send_settings(
        vcard_keys: Option<&PinnedPublicKeys<Pub>>,
        mail_settings: &CryptoMailSettings,
        is_internal: bool,
    ) -> (PackageMimeType, bool, bool, CryptoPackageType) {
        // Try to extract the mime type from the vcard keys,
        // fallback to the mailsettings if not information can be extracted.
        let mut send_settings_mime_type = vcard_keys
            .as_ref()
            .and_then(|pinned_keys| pinned_keys.mime_type.map(Into::into))
            .unwrap_or(mail_settings.mime_type);

        // Try to extract information if encryption should be enabled;.
        // With pinned keys, an undefined flag also defaults to true
        // (because in the past, we did not store the encryption flag in the contact when pinning keys from WKD)
        let send_settings_encrypt = vcard_keys
            .and_then(|pinned_keys| pinned_keys.encrypt_to_pinned)
            .unwrap_or(true);

        // Try to extract information if sign should be enabled,
        // fallback to the mailsettings if not information can be extracted.
        let send_settings_sign = vcard_keys
            .and_then(|pinned_keys| pinned_keys.sign)
            .unwrap_or(mail_settings.sign);

        // Try to extract information of the pgp mode,
        // fallback to the mailsettings if not information can be extracted.
        let mut send_settings_pgp_type = if is_internal {
            CryptoPackageType::ProtonMail
        } else {
            vcard_keys
                .and_then(|pinned_keys| pinned_keys.scheme)
                .map_or(
                    CryptoPackageType::from_scheme(
                        mail_settings.pgp_scheme,
                        send_settings_encrypt,
                        send_settings_sign,
                    ),
                    |scheme| {
                        CryptoPackageType::from_scheme(
                            scheme,
                            send_settings_encrypt,
                            send_settings_sign,
                        )
                    },
                )
        };

        if send_settings_pgp_type == CryptoPackageType::PgpInline {
            // We do not support inline, so overwrite it with mime.
            send_settings_pgp_type = CryptoPackageType::PgpMime;
        }

        if send_settings_pgp_type == CryptoPackageType::PgpMime
            || send_settings_pgp_type == CryptoPackageType::ClearMime
        {
            // Force multipart mime type for PGP Mime mode.
            send_settings_mime_type = PackageMimeType::Multipart;
        }

        (
            send_settings_mime_type,
            send_settings_encrypt,
            send_settings_sign,
            send_settings_pgp_type,
        )
    }
}
