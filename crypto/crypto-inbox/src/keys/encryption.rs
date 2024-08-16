//! [Confluence Sending Preferences](https://confluence.protontech.ch/display/MAILFE/Send+preferences+for+outgoing+email)
//! [Advanced encryption setting](https://confluence.protontech.ch/display/MAILFE/Advanced+PGP+settings)

use proton_crypto_account::{
    keys::{
        ContactType, DecryptedAddressKey, EmailMimeType, InboxPublicKeys, PGPScheme,
        PinnedPublicKeys, RecipientPublicKeyModel,
    },
    proton_crypto::{
        crypto::{PrivateKey, PublicKey, UnixTimestamp},
        keytransparency::KTVerificationResult,
    },
};

use crate::message::packages::PackageMimeType;

use super::{CryptoPackageTypeError, EncryptionPreferencesError};

/// A helper type that contains the default PGP preferences
/// extracted from the user's mailsettings.
#[derive(Debug, Default, PartialEq, Eq, Copy, Clone, Hash)]
pub struct CryptoMailSettings {
    /// The default PGP scheme to use.
    pub pgp_scheme: PGPScheme,

    /// The default content mime type to use.
    pub mime_type: EmailMimeType,

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
    /// See [confluence](https://proton.me/support/password-protected-emails).
    pub encrypt_to_outside: bool,
}

/// All possible encryption types as requested by the Proton API.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum PackageCryptoType {
    /// Encrypted using `ProtonMail`'s native encryption.
    ProtonMail,

    /// Encrypted with a password for users outside of `ProtonMail`'s system.
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

impl PackageCryptoType {
    pub fn type_value(&self) -> i32 {
        match self {
            PackageCryptoType::ProtonMail => 1,
            PackageCryptoType::EncryptedOutside => 2,
            PackageCryptoType::Cleartext => 4,
            PackageCryptoType::PgpInline => 8,
            PackageCryptoType::PgpMime => 16,
            PackageCryptoType::ClearMime => 32,
        }
    }

    pub fn enum_of(value: i32) -> Option<PackageCryptoType> {
        match value {
            1 => Some(PackageCryptoType::ProtonMail),
            2 => Some(PackageCryptoType::EncryptedOutside),
            4 => Some(PackageCryptoType::Cleartext),
            8 => Some(PackageCryptoType::PgpInline),
            16 => Some(PackageCryptoType::PgpMime),
            32 => Some(PackageCryptoType::ClearMime),
            _ => None,
        }
    }

    pub fn from_scheme(scheme: PGPScheme, encrypt: bool, sign: bool) -> PackageCryptoType {
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

impl TryFrom<i32> for PackageCryptoType {
    type Error = CryptoPackageTypeError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
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

    /// The MIME type of the email, specifying the format of the email body.
    pub mime_type: EmailMimeType,

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
    /// # Parameters
    ///
    /// - `recipient_key_model`: A `RecipientPublicKeyModel<Pub>` containing the recipient's public key
    ///   information, contact type, and other relevant details. This model is used to determine the best
    ///   encryption key and preferences for the recipient.
    /// - `crypto_mail_settings`: A reference to `CryptoMailSettings` that holds the user's default settings
    ///   for signing and PGP scheme. These settings are used as fallbacks when the recipient-specific preferences
    ///   are not fully defined.
    ///
    /// # Errors
    ///
    /// A [`EncryptionPreferencesError`] if the key selection fails.
    pub fn create_from(
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
        let mime_type = recipient_key_model
            .mime_type
            .unwrap_or(crypto_mail_settings.mime_type);

        // Select the `OpenPGP` public key based on the recipient type.
        let (selected_key, is_selected_key_pinned) = match recipient_key_model.contact_type {
            ContactType::Internal => {
                encrypt = true;
                sign = true;
                Self::select_key_for_recipient_with_api_keys_internal(&recipient_key_model)?
            }
            ContactType::ExternalWithApiKeys => {
                Self::select_key_for_recipient_with_api_keys_external(&recipient_key_model)?
            }
            ContactType::InternalWithDisabledE2EEForMail => {
                encrypt = false;
                sign = false;
                (None, false)
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
    /// # Parameters
    ///
    /// - `address_keys`: A slice of `DecryptedAddressKey<Priv, Pub>` representing the user's own address keys.
    ///   The function iterates through these keys to find a valid primary key for encryption.
    /// - `mail_settings`: A reference to `CryptoMailSettings` containing the user's default settings for PGP
    ///   scheme and MIME type. These settings are applied to the generated `EncryptionPreferences`.
    /// - `encryption_time`: A `UnixTimestamp` representing the current time used to validate the encryption
    ///   capability of the keys.
    ///
    /// # Errors
    ///
    /// This function may return an [`EncryptionPreferencesError::NoPrimaryKey`] if no valid primary key
    /// is found in the user's address keys that meets the required conditions for encryption.
    fn create_from_self<Priv: PrivateKey>(
        address_keys: &[DecryptedAddressKey<Priv, Pub>],
        mail_settings: CryptoMailSettings,
        encryption_time: UnixTimestamp,
    ) -> Result<Self, EncryptionPreferencesError> {
        // Select a valid primary key in the address.
        let Some(selected_key) = address_keys.iter().find(|address_key| {
            address_key.primary
                && !address_key.flags.is_compromised()
                && !address_key.flags.is_obsolete()
                && address_key.public_key.can_encrypt(encryption_time)
        }) else {
            return Err(EncryptionPreferencesError::NoPrimaryKey);
        };
        Ok(EncryptionPreferences {
            encrypt: true,
            sign: true,
            contact_type: ContactType::Internal,
            pgp_scheme: mail_settings.pgp_scheme,
            mime_type: mail_settings.mime_type,
            selected_key: Some(selected_key.public_key.clone()),
            is_selected_key_pinned: false,
            key_transparency_verification: Ok(()),
        })
    }

    /// Helper function to select the encryption key for an internal recipient with API keys.
    fn select_key_for_recipient_with_api_keys_internal(
        recipient_key_model: &RecipientPublicKeyModel<Pub>,
    ) -> Result<(Option<&Pub>, bool), EncryptionPreferencesError> {
        // Take the first API key. They are ordered according to their validity and preference.
        // Trusted keys have higher priority but might no be able to encrypt.
        let Some(selected_key) = recipient_key_model.api_keys.first() else {
            return Err(EncryptionPreferencesError::InternalUserNoApiKeys);
        };

        // Check if the key can be used to encrypt and send an email.
        if !recipient_key_model.is_selected_key_valid_for_sending(selected_key) {
            return Err(EncryptionPreferencesError::PrimaryKeyCannotSend(
                selected_key.key_fingerprint(),
                recipient_key_model.is_selected_key_obsolete(selected_key),
                recipient_key_model.is_selected_key_compromised(selected_key),
                recipient_key_model.can_selected_key_encrypt(selected_key),
            ));
        }

        // Check for pinned keys.
        if !recipient_key_model.pinned_keys.is_empty() {
            let primary_trusted = recipient_key_model.is_selected_key_trusted(selected_key);
            let primary_fingerprint = selected_key.key_fingerprint();
            if !primary_trusted {
                return Err(EncryptionPreferencesError::PrimaryKeyNotPinned(
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

    /// Helper function to select the encryption key for an external recipient with API keys.
    fn select_key_for_recipient_with_api_keys_external(
        recipient_key_model: &RecipientPublicKeyModel<Pub>,
    ) -> Result<(Option<&Pub>, bool), EncryptionPreferencesError> {
        let Some(selected_key) = recipient_key_model.api_keys.first() else {
            return Err(EncryptionPreferencesError::ExternalUserNoValidApiKey);
        };

        let Some(valid_api_send_key) = recipient_key_model
            .api_keys
            .iter()
            .find(|public_key| recipient_key_model.can_selected_key_encrypt(public_key))
        else {
            return Err(EncryptionPreferencesError::ExternalUserNoValidApiKey);
        };

        // Check for pinned keys.
        if !recipient_key_model.pinned_keys.is_empty() {
            let primary_trusted_and_valid = recipient_key_model
                .is_selected_key_trusted(selected_key)
                && recipient_key_model.is_selected_key_valid_for_sending(selected_key);

            if !primary_trusted_and_valid {
                return Err(EncryptionPreferencesError::PrimaryKeyNotPinned(
                    valid_api_send_key.key_fingerprint(),
                ));
            }

            let primary_fingerprint = selected_key.key_fingerprint();
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

impl<Pub: PublicKey> SendPreferences<Pub> {
    /// Creates an instance of [`SendPreferences`] based on the given encryption preferences and composer settings input.
    ///
    /// This function determines the final send preferences for an email by combining the provided encryption
    /// preferences with additional settings from the composer. It derives the package mode of encryption and the
    /// MIME type of the package body.
    /// See [confluence](https://confluence.protontech.ch/display/MAILFE/Send+preferences+for+outgoing+email) for more details on the logic.
    ///
    /// # Parameters
    ///
    /// - `encryption_preferences`: An `EncryptionPreferences<Pub>` instance that contains the initial encryption
    ///   and signing preferences, PGP scheme, MIME type, and details about the selected public key and key
    ///   transparency verification.
    /// - `encrypt_to_outside`: A `bool` indicating that the user has enable encrypt to outside in the composer.
    /// - `composer_sign`: A `bool` indicating whether the email should be signed based on the user's choice in the composer.
    pub fn create_from(
        encryption_preferences: EncryptionPreferences<Pub>,
        composer_preferences: ComposerPreference,
    ) -> Self {
        let encrypt = encryption_preferences.encrypt || composer_preferences.encrypt_to_outside;
        let sign = composer_preferences.encrypt_to_outside || encryption_preferences.sign;

        // Select the encryption mode (PackageCryptoType) for the package sent to this recipient.
        let pgp_scheme = match encryption_preferences.contact_type {
            ContactType::Internal => PackageCryptoType::ProtonMail,
            ContactType::InternalWithDisabledE2EEForMail => PackageCryptoType::Cleartext,
            ContactType::ExternalWithApiKeys | ContactType::ExternalWithNoApiKeys => {
                let scheme = PackageCryptoType::from_scheme(
                    encryption_preferences.pgp_scheme,
                    encrypt,
                    sign,
                );
                // Force PGP mime as inline pgp is not supported currently
                if scheme == PackageCryptoType::PgpInline {
                    PackageCryptoType::PgpMime
                } else {
                    scheme
                }
            }
        };

        // Select the mime type of email body.
        let mime_type = match pgp_scheme {
            PackageCryptoType::PgpInline => PackageMimeType::Text,
            PackageCryptoType::PgpMime | PackageCryptoType::ClearMime => PackageMimeType::Multipart,
            _ => encryption_preferences.mime_type.into(),
        };

        Self {
            encrypt,
            sign,
            pgp_scheme,
            mime_type,
            selected_key: encryption_preferences.selected_key,
            is_selected_key_pinned: encryption_preferences.is_selected_key_pinned,
            encryption_disabled: encryption_preferences.contact_type
                == ContactType::InternalWithDisabledE2EEForMail,
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
    /// # Parameters
    ///
    /// - `api_keys`: The `InboxPublicKeys<Pub>` containing the recipient's public keys.
    /// - `pinned_keys`: An optional `PinnedPublicKeys<Pub>` representing additional encryption key preferences from a v-card.
    /// - `encryption_time`: The `UnixTimestamp` representing the current time for validating the `OpenPGP` keys.
    /// - `crypto_mail_settings`: A reference to `CryptoMailSettings` defining the user's default encryption and signing settings.
    /// - `encrypt_to_outside`: A `bool` indicating that the user has enabled encrypt to outside explicitly in the composer. `false` is default.
    /// - `composer_sign`: A `bool` indicating whether the email should be signed based on the user's choice in the composer. `false` is default.
    ///
    ///
    /// # Errors
    ///
    /// A [`EncryptionPreferencesError`] if the key selection fails.
    pub fn new(
        api_keys: InboxPublicKeys<Pub>,
        pinned_keys: Option<PinnedPublicKeys<Pub>>,
        encryption_time: UnixTimestamp,
        crypto_mail_settings: &CryptoMailSettings,
        composer_preferences: ComposerPreference,
    ) -> Result<Self, EncryptionPreferencesError> {
        let recipient_key_model =
            RecipientPublicKeyModel::create_from(api_keys, pinned_keys, encryption_time);

        let encryption_preferences =
            EncryptionPreferences::create_from(recipient_key_model, crypto_mail_settings)?;

        Ok(SendPreferences::create_from(
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
    /// # Parameters
    ///
    /// - `address_keys`: A slice of `DecryptedAddressKey<Priv, Pub>` containing the decrypted address keys of the user.
    /// - `encryption_time`: A `UnixTimestamp` representing the current time, used to validate the `OpenPGP` key.
    /// - `crypto_mail_settings`: A reference to `CryptoMailSettings` defining the user's default encryption and signing settings.
    ///
    /// # Errors
    ///
    /// - [`EncryptionPreferencesError::NoPrimaryKey`] - If no valid primary key can be selected from the address keys.
    pub fn new_self<Priv: PrivateKey>(
        address_keys: &[DecryptedAddressKey<Priv, Pub>],
        encryption_time: UnixTimestamp,
        crypto_mail_settings: CryptoMailSettings,
    ) -> Result<Self, EncryptionPreferencesError> {
        let encryption_preferences = EncryptionPreferences::create_from_self(
            address_keys,
            crypto_mail_settings,
            encryption_time,
        )?;

        Ok(SendPreferences::create_from(
            encryption_preferences,
            ComposerPreference::default(),
        ))
    }
}

impl<Pub: PublicKey> From<EncryptionPreferences<Pub>> for SendPreferences<Pub> {
    fn from(value: EncryptionPreferences<Pub>) -> Self {
        Self::create_from(value, ComposerPreference::default())
    }
}
