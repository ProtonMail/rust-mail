//! This module is responsible for determining the preferences used when sending an email to a specific address.
//! These preferences include whether the email should be encrypted or signed, which public key to use if encryption is needed,
//! and the appropriate PGP scheme and email format (such as the MIME type of the message).
//!
//! The implementation of this module follows the guidelines detailed in the
//! [Confluence Sending Preferences](https://confluence.protontech.ch/display/MAILFE/Send+preferences+for+outgoing+email).
//!
//! It is important to note that this logic is inherently complex, involving multiple steps and numerous edge cases.
//! To ensure consistent behavior and avoid discrepancies, we have chosen to closely mirror the implementation found in the web inbox implementation.
//!
//! In future iterations, we may explore opportunities to simplify this code, keeping it in sync with potential changes in the web implementation.

use std::fmt::Display;

use proton_crypto_account::{
    keys::{
        ContactType, DecryptedAddressKey, EmailMimeType, PGPScheme, PinnedPublicKeys,
        PublicAddressKeys, RecipientPublicKeyModel,
    },
    proton_crypto::{
        crypto::{PrivateKey, PublicKey, UnixTimestamp},
        keytransparency::KTVerificationResult,
    },
};
use serde_repr::Serialize_repr;

use crate::message::packages::PackageMimeType;

use super::{CryptoPackageTypeError, EncryptionPreferencesError};

/// A helper type that contains the default PGP preferences
/// extracted from the user's mailsettings.
#[derive(Debug, Default, PartialEq, Eq, Copy, Clone, Hash)]
pub struct CryptoMailSettings {
    /// The default PGP scheme to use.
    pub pgp_scheme: PGPScheme,

    /// If mails should be signed by default.
    pub sign: bool,
}

/// Contains the preferences a user can select in the composer to
/// change the behavior of email encryption.
#[derive(Debug, Default, PartialEq, Eq, Copy, Clone, Hash)]
pub struct ComposerPreference {
    /// Indicates if encrypt to outside is enabled.
    ///
    /// The Encrypt to outside (EO) mode encrypts email with a password.
    /// See [web](https://proton.me/support/password-protected-emails).
    pub encrypt_to_outside: bool,

    /// The mime type of the message in the composer.
    pub composer_body_mime_type: EmailMimeType,
}

impl ComposerPreference {
    #[must_use]
    pub fn new(composer_body_mime_type: EmailMimeType) -> Self {
        Self {
            encrypt_to_outside: false,
            composer_body_mime_type,
        }
    }
}

/// All possible encryption types as requested by the Proton API.

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, Hash, Serialize_repr)]
#[repr(u8)]
pub enum PackageCryptoType {
    /// Encrypted using `ProtonMail`'s native encryption.
    ProtonMail = 1,

    /// Encrypted with a password for users outside of `ProtonMail`'s system.
    EncryptedOutside = 2,

    /// Message is not encrypted and is in plain text.
    #[default]
    Cleartext = 4,

    /// PGP encryption using inline PGP format.
    PgpInline = 8,

    /// PGP encryption using MIME format.
    PgpMime = 16,

    /// Cleartext message with MIME formatting.
    ClearMime = 32,
}

impl Display for PackageCryptoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageCryptoType::ProtonMail => f.write_str("proton mail"),
            PackageCryptoType::EncryptedOutside => f.write_str("encrypt to outside"),
            PackageCryptoType::Cleartext => f.write_str("cleartext"),
            PackageCryptoType::PgpInline => f.write_str("pgp inline"),
            PackageCryptoType::PgpMime => f.write_str("pgp mime"),
            PackageCryptoType::ClearMime => f.write_str("pgp clear mime"),
        }
    }
}

impl PackageCryptoType {
    #[must_use]
    pub fn type_value(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn enum_of(value: u8) -> Option<PackageCryptoType> {
        Self::try_from(value).ok()
    }

    #[must_use]
    pub fn from_scheme(
        scheme: PGPScheme,
        encrypt: bool,
        sign: bool,
        eo: bool,
    ) -> PackageCryptoType {
        if eo {
            return PackageCryptoType::EncryptedOutside;
        }
        match scheme {
            PGPScheme::PGPMime => {
                if !encrypt && sign {
                    PackageCryptoType::ClearMime
                } else if encrypt {
                    PackageCryptoType::PgpMime
                } else {
                    PackageCryptoType::Cleartext
                }
            }
            PGPScheme::PGPInline => {
                if encrypt {
                    PackageCryptoType::PgpInline
                } else {
                    PackageCryptoType::Cleartext
                }
            }
        }
    }
}

impl TryFrom<u8> for PackageCryptoType {
    type Error = CryptoPackageTypeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::enum_of(value).ok_or(CryptoPackageTypeError::Parse(value))
    }
}

/// Represents the encryption preferences for sending an email, including options for encryption, signing,
/// PGP scheme, MIME type, and selected public key.
///
/// This struct encapsulates the settings and choices made when preparing an email for sending,
/// specifically focusing on whether the email should be encrypted or signed, and which PGP scheme and
/// MIME type to use. It also includes the selected public key for encryption and additional metadata
/// about the selection process.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools, clippy::module_name_repetitions)]
pub struct EncryptionPreferences<Pub: PublicKey> {
    /// Indicates whether the email should be encrypted (`true`) or sent unencrypted (`false`).
    ///
    /// If `true`, the email content will be encrypted using the selected public key. If `false`,
    /// the email will be sent in plaintext.
    pub encrypt: bool,

    /// Indicates whether the email should be signed (`true`) or sent unsigned (`false`).
    ///
    /// If `true`, the email will be signed with the sender's private key, allowing the recipient
    /// to verify the authenticity and integrity of the message.
    pub sign: bool,

    /// The type of contact, which influences the default encryption and signing behavior.
    ///
    /// This field differentiates between internal and external contacts, which may have different
    /// default settings for encryption and signing. For instance, internal contacts might always
    /// require encryption, while external contacts might have more flexible settings.
    pub contact_type: ContactType,

    /// The `OpenPGP` scheme to use when encrypting the email to an external recipient.
    pub pgp_scheme: PGPScheme,

    /// An optional preference for the MIME type of the body.
    pub mime_type: Option<EmailMimeType>,

    /// Optionally stores the selected public key for encryption.
    ///
    /// This field contains the public key that will be used to encrypt the email content if
    /// encryption is enabled. It is `None` if encryption is not required or if no suitable
    /// public key was found.
    pub selected_key: Option<Pub>,

    /// Indicates whether the selected key is pinned.
    ///
    /// A pinned key is one that has been manually selected/trusted and, thus, the security of the key does
    /// not rely on trusting the server serving the right key.
    pub is_selected_key_pinned: bool,

    /// Indicates that the receiving address wants encryption disabled although
    /// being an proton internal address.
    pub encryption_disabled_mail: bool,

    /// Result of the key transparency verification process for API keys.
    pub key_transparency_verification: KTVerificationResult,
}

impl<Pub: PublicKey> EncryptionPreferences<Pub> {
    /// Creates an instance of [`EncryptionPreferences`] by determining the appropriate encryption and signing
    /// settings based on the recipient's public key model and the user's cryptographic mail settings.
    ///
    /// This function analyzes the recipient's public key information, the type of recipient, and the user's
    /// default mail settings to decide whether the email should be encrypted and/or signed. It also selects
    /// the most appropriate PGP scheme and MIME type for the email and identifies the public key to use for
    /// encryption, if applicable.
    /// See [confluence](https://confluence.protontech.ch/display/MAILFE/Send+preferences+for+outgoing+email) for more details on the logic.
    ///
    /// # Errors
    ///
    /// An [`EncryptionPreferencesError`] if the key selection fails.
    /// An [`EncryptionPreferencesError::ApiKeyNotPinned`] is thrown if there are pinned keys, but none of the fingerprints of the pinned keys matches
    /// the fingerprint of one of the keys served by the API.
    /// In this case the client should force the user (via a modal) to trust one of the keys served by the API before sending any email.
    pub fn from_key_model_and_settings(
        recipient_key_model: RecipientPublicKeyModel<Pub>,
        crypto_mail_settings: &CryptoMailSettings,
    ) -> Result<Self, EncryptionPreferencesError> {
        // Determine the PGP preferences and fallback to the mail settings if not set.
        let mut encrypt = recipient_key_model.encrypt.unwrap_or_default();
        let mut sign = recipient_key_model
            .sign
            .unwrap_or(crypto_mail_settings.sign);
        sign = encrypt || sign;
        let scheme = recipient_key_model
            .pgp_scheme
            .unwrap_or(crypto_mail_settings.pgp_scheme);
        let mime_type = recipient_key_model.mime_type;

        // Select the `OpenPGP` public key based on the recipient type.
        let (selected_key, is_selected_key_pinned) = match recipient_key_model.contact_type {
            ContactType::Internal => {
                encrypt = true;
                sign = true;
                Self::select_key_for_recipient_with_api_keys(&recipient_key_model)?
            }
            ContactType::ExternalWithApiKeys => {
                Self::select_key_for_recipient_with_api_keys(&recipient_key_model)?
            }
            ContactType::ExternalWithNoApiKeys => {
                Self::select_key_for_recipient_without_api_keys(&recipient_key_model, encrypt)?
            }
        };

        Ok(EncryptionPreferences {
            encrypt,
            sign,
            contact_type: recipient_key_model.contact_type,
            pgp_scheme: scheme,
            mime_type,
            selected_key: selected_key.cloned(),
            is_selected_key_pinned,
            encryption_disabled_mail: recipient_key_model.is_internal_with_disabled_e2ee,
            key_transparency_verification: recipient_key_model.key_transparency_verification,
        })
    }

    /// Creates an instance of `EncryptionPreferences` for sending an email to the user's own address
    /// by selecting the appropriate encryption and signing settings based on the provided address keys
    /// and mail settings.
    ///
    /// This function determines the encryption and signing preferences by selecting a valid primary key
    /// from the user's own address keys. The selected key must be capable of encryption, not compromised,
    /// and not obsolete. The function uses the user's mail settings to configure the PGP scheme and MIME type
    /// for the email.
    ///
    /// # Errors
    ///
    /// This function may return an [`EncryptionPreferencesError::NoPrimaryKey`] if no valid primary key
    /// is found in the user's address keys that meets the required conditions for encryption.
    fn from_unlocked_address_keys_and_settings<Priv: PrivateKey>(
        address_keys: &[DecryptedAddressKey<Priv, Pub>],
        mail_settings: CryptoMailSettings,
        encryption_time: UnixTimestamp,
    ) -> Result<Self, EncryptionPreferencesError> {
        // Select a valid primary key in the address.
        let selected_key_v4_opt = address_keys.iter().find(|address_key| {
            address_key.primary
                && !address_key.flags.is_compromised()
                && !address_key.flags.is_obsolete()
                && address_key.public_key.can_encrypt(encryption_time)
                && !address_key.is_v6
        });

        // If there is a valid v6 primary key, prefer it for encryption.
        let selected_key_v6_opt = address_keys.iter().find(|address_key| {
            address_key.primary
                && !address_key.flags.is_compromised()
                && !address_key.flags.is_obsolete()
                && address_key.public_key.can_encrypt(encryption_time)
                && address_key.is_v6
        });
        let selected_key = match (selected_key_v4_opt, selected_key_v6_opt) {
            (None, None) => return Err(EncryptionPreferencesError::NoPrimaryKey),
            (None | Some(_), Some(selected_key_v6)) => &selected_key_v6.public_key,
            (Some(selected_key_v4), None) => &selected_key_v4.public_key,
        };

        Ok(EncryptionPreferences {
            encrypt: true,
            sign: true,
            contact_type: ContactType::Internal,
            pgp_scheme: mail_settings.pgp_scheme,
            mime_type: None,
            selected_key: Some(selected_key.clone()),
            is_selected_key_pinned: false,
            encryption_disabled_mail: false,
            key_transparency_verification: Ok(()),
        })
    }

    /// Helper function to select the encryption key for an internal or external recipient with API keys.
    fn select_key_for_recipient_with_api_keys(
        recipient_key_model: &RecipientPublicKeyModel<Pub>,
    ) -> Result<(Option<&Pub>, bool), EncryptionPreferencesError> {
        let is_external = recipient_key_model.contact_type != ContactType::Internal;
        // Take the first API key. They are ordered according to their validity and preference.
        // Pinned keys (trusted) have higher priority.
        // For an external user at most one API key (from WKD or KOO) will be returned by the server.
        // So, we again just take the first one.
        let Some(selected_key) = recipient_key_model.api_keys.first() else {
            return if is_external {
                Err(EncryptionPreferencesError::ExternalUserNoValidApiKey)
            } else {
                Err(EncryptionPreferencesError::InternalUserNoApiKeys)
            };
        };

        // Check if the key can be used to encrypt and send an email.
        if !recipient_key_model.is_selected_key_valid_for_sending(selected_key) {
            return Err(EncryptionPreferencesError::SelectedKeyCannotSend(
                recipient_key_model.contact_type,
                selected_key.key_fingerprint(),
                recipient_key_model.is_selected_key_obsolete(selected_key),
                recipient_key_model.is_selected_key_compromised(selected_key),
                recipient_key_model.can_selected_key_encrypt(selected_key),
            ));
        }

        // Check for pinned keys.
        if !recipient_key_model.pinned_keys.is_empty() {
            // The client should encrypt the email with the first pinned key whose fingerprint matches the fingerprint
            // of one of the keys served by the API.
            // The keys in the vCard should be ordered according to their PREF
            // property if that has not been specified they are taken in the order in which they are written in the vCard.
            let primary_fingerprint = selected_key.key_fingerprint();
            if !recipient_key_model.is_selected_key_trusted(selected_key) {
                return Err(EncryptionPreferencesError::PinnedKeyNotProvidedByAPI(
                    primary_fingerprint,
                ));
            }
            let pinned_key = recipient_key_model
                .pinned_keys
                .iter()
                .find(|key| key.key_fingerprint() == primary_fingerprint)
                .unwrap_or(selected_key); // There must always be a match if the primary is trusted.
            return Ok((Some(pinned_key), true));
        }
        Ok((Some(selected_key), false))
    }

    /// Helper function to select the encryption key for an external
    /// recipient with no API keys.
    fn select_key_for_recipient_without_api_keys(
        recipient_key_model: &RecipientPublicKeyModel<Pub>,
        encrypt: bool,
    ) -> Result<(Option<&Pub>, bool), EncryptionPreferencesError> {
        // Pinned keys are sorted according to their validity.
        // The first valid one (as stored in the vCard) should be used.
        let Some(pinned_key) = recipient_key_model.pinned_keys.first() else {
            return Ok((None, false));
        };
        if !encrypt {
            return Ok((None, false));
        }
        if !recipient_key_model.is_selected_key_valid_for_sending(pinned_key) {
            return Err(EncryptionPreferencesError::ExternalUserNoValidPinnedKey(
                pinned_key.key_fingerprint(),
                recipient_key_model.is_selected_key_obsolete(pinned_key),
                recipient_key_model.is_selected_key_compromised(pinned_key),
                recipient_key_model.can_selected_key_encrypt(pinned_key),
            ));
        }
        Ok((Some(pinned_key), true))
    }
}

/// Represents the preferences for sending an email, including encryption, signing, and formatting options.
///
/// This type is used to determine the Package for encrypting and sending an email to a recipient.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct SendPreferences<Pub: PublicKey> {
    /// Indicates whether the email should be encrypted (`true`) or sent unencrypted (`false`).
    ///
    /// If `true`, the email content will be encrypted using the selected public key. If `false`,
    /// the email will be sent in plaintext.
    pub encrypt: bool,

    /// Indicates whether the email should be signed (`true`) or sent unsigned (`false`).
    ///
    /// If `true`, the email will be signed with the sender's private key, allowing the recipient
    /// to verify the authenticity and integrity of the message.
    pub sign: bool,

    /// Specifies the Proton encryption scheme to be used in the email package.
    pub pgp_scheme: PackageCryptoType,

    /// Specifies the MIME type for formatting the email body in each email package.
    pub mime_type: PackageMimeType,

    /// Optionally stores the selected public key for encryption.
    ///
    /// This field contains the public key that will be used to encrypt the email content if
    /// encryption is enabled. It is `None` if encryption is not required or if no suitable
    /// public key was found.
    pub selected_key: Option<Pub>,

    /// Indicates whether the selected key is pinned.
    ///
    /// A pinned key is one that has been manually selected/trusted and, thus, the security of the key does
    /// not rely on trusting the server serving the right key.
    pub is_selected_key_pinned: bool,

    /// Indicates whether an internal user has disabled encryption explicitly.
    ///
    /// See [here](https://proton.me/support/manage-encryption) how to configure this.
    pub encryption_disabled: bool,

    /// Result of the key transparency verification process.
    pub key_transparency_verification: KTVerificationResult,
}

impl<Pub: PublicKey> Display for SendPreferences<Pub> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(encrypt={} sign={}", self.encrypt, self.sign)?;
        write!(f, " pgp_scheme=\"{}\"", self.pgp_scheme)?;
        write!(f, " mime_type={}", self.mime_type)?;
        write!(f, " has_key={}", self.selected_key.is_some())?;
        write!(f, " pinned={}", self.is_selected_key_pinned)?;
        write!(f, " encryption_disabled={})", self.encryption_disabled)
    }
}

impl<Pub: PublicKey> SendPreferences<Pub> {
    /// Creates an instance of [`SendPreferences`] based on the given encryption preferences and composer settings input.
    ///
    /// This function determines the final send preferences for an email by combining the provided encryption
    /// preferences with additional settings from the composer. It derives the package mode of encryption and the
    /// MIME type of the package body.
    /// See [confluence](https://confluence.protontech.ch/display/MAILFE/Send+preferences+for+outgoing+email) for more details on the logic.
    pub fn from_preferences(
        encryption_preferences: EncryptionPreferences<Pub>,
        composer_preferences: ComposerPreference,
    ) -> Self {
        let encrypt;
        let sign;

        if encryption_preferences.encryption_disabled_mail {
            encrypt = false;
            sign = false;
        } else {
            encrypt = encryption_preferences.encrypt || composer_preferences.encrypt_to_outside;
            sign = composer_preferences.encrypt_to_outside || encryption_preferences.sign;
        }

        // Select encryption mode for the package.
        //
        // Note that we can't dispatch `ProtonMail` packages if the encryption
        // is disabled for that recipient - that's going to be the case for
        // emails that are forwarded from a Proton address to a non-Proton one.
        //
        // (if we'd encrypted that message, the server wouldn't be able to
        // forward it, since it wouldn't know how to perform the decryption.)
        let pgp_scheme = if encryption_preferences.contact_type == ContactType::Internal
            && !encryption_preferences.encryption_disabled_mail
        {
            PackageCryptoType::ProtonMail
        } else {
            let scheme = PackageCryptoType::from_scheme(
                encryption_preferences.pgp_scheme,
                encrypt,
                sign,
                composer_preferences.encrypt_to_outside,
            );

            // Force PGP mime as inline pgp is not supported currently
            if scheme == PackageCryptoType::PgpInline {
                PackageCryptoType::PgpMime
            } else {
                scheme
            }
        };

        // Select the mime type of email body.
        let mime_type = match pgp_scheme {
            PackageCryptoType::PgpInline => PackageMimeType::Text,
            PackageCryptoType::PgpMime | PackageCryptoType::ClearMime => PackageMimeType::Multipart,
            // If sending EO, respect the MIME type of the composer, since it will be what the API returns when retrieving the message.
            PackageCryptoType::EncryptedOutside => {
                composer_preferences.composer_body_mime_type.into()
            }
            PackageCryptoType::Cleartext | PackageCryptoType::ProtonMail => {
                // composer html -> contact text = text
                // composer text -> contact html = text
                // composer html -> contact auto/none = html
                // composer text -> contact auto/none = text
                match composer_preferences.composer_body_mime_type {
                    EmailMimeType::Text => composer_preferences.composer_body_mime_type.into(), // Prefer composer preference if message body is text/plain
                    EmailMimeType::Html => encryption_preferences
                        .mime_type
                        .unwrap_or(composer_preferences.composer_body_mime_type)
                        .into(),
                }
            }
        };

        Self {
            encrypt,
            sign,
            pgp_scheme,
            mime_type,
            selected_key: encryption_preferences.selected_key,
            is_selected_key_pinned: encryption_preferences.is_selected_key_pinned,
            encryption_disabled: encryption_preferences.encryption_disabled_mail,
            key_transparency_verification: encryption_preferences.key_transparency_verification,
        }
    }

    /// Creates a new [`SendPreferences`] instance by first determining the recipient's public key model,
    /// then deriving the encryption preferences from it, and finally adjusting the send preferences based on
    /// additional parameters.
    ///
    /// This function orchestrates the process of generating the appropriate send preferences for an email by
    /// utilizing the recipient's public keys, the user's mail settings, and various flags related to encryption
    /// and signing.
    /// See [confluence](https://confluence.protontech.ch/display/MAILFE/Send+preferences+for+outgoing+email) for more details on the logic.
    ///
    /// # Errors
    ///
    /// An [`EncryptionPreferencesError`] if the key selection fails.
    /// An [`EncryptionPreferencesError::ApiKeyNotPinned`] is thrown if there are pinned keys, but none of the fingerprints of the pinned keys matches
    /// the fingerprint of one of the keys served by the API.
    /// In this case the client should force the user (via a modal) to trust one of the keys served by the API before sending any email.
    pub fn new(
        api_keys: PublicAddressKeys<Pub>,
        pinned_keys: Option<PinnedPublicKeys<Pub>>,
        encryption_time: UnixTimestamp,
        crypto_mail_settings: &CryptoMailSettings,
        composer_preferences: ComposerPreference,
    ) -> Result<Self, EncryptionPreferencesError> {
        let recipient_key_model = RecipientPublicKeyModel::from_public_keys_at_time(
            api_keys,
            pinned_keys,
            encryption_time,
            true, // prefer v6/pqc keys for mail.
        );

        let encryption_preferences = EncryptionPreferences::from_key_model_and_settings(
            recipient_key_model,
            crypto_mail_settings,
        )?;

        Ok(SendPreferences::from_preferences(
            encryption_preferences,
            composer_preferences,
        ))
    }

    /// Creates a new [`SendPreferences`] instance for an internal sender by determining the encryption preferences
    /// based on the sender's address keys and the provided mail settings.
    ///
    /// This function is specifically designed for creating send preferences for users sending to themselves, where encryption and
    /// signing are generally enabled by default.
    ///
    /// # Errors
    ///
    /// - [`EncryptionPreferencesError::NoPrimaryKey`] - If no valid primary key can be selected from the address keys.
    pub fn new_for_self<Priv: PrivateKey>(
        address_keys: &[DecryptedAddressKey<Priv, Pub>],
        encryption_time: UnixTimestamp,
        crypto_mail_settings: CryptoMailSettings,
        composer_preferences: ComposerPreference,
    ) -> Result<Self, EncryptionPreferencesError> {
        let encryption_preferences =
            EncryptionPreferences::from_unlocked_address_keys_and_settings(
                address_keys,
                crypto_mail_settings,
                encryption_time,
            )?;

        Ok(SendPreferences::from_preferences(
            encryption_preferences,
            composer_preferences,
        ))
    }
}

impl<Pub: PublicKey> From<EncryptionPreferences<Pub>> for SendPreferences<Pub> {
    fn from(value: EncryptionPreferences<Pub>) -> Self {
        Self::from_preferences(value, ComposerPreference::default())
    }
}
