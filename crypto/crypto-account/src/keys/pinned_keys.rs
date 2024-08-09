use std::str::FromStr;

use proton_crypto::crypto::{PublicKey, UnixTimestamp};

use super::{InboxPublicKeys, RecipientType};

/// Error returned if parsing the [`PGPScheme`] from a string fails.
#[derive(Debug, PartialEq, Eq)]
pub struct ParsePGPSchemeError;

/// PGP scheme options to encrypt and email.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PGPScheme {
    PGPInline,
    #[default]
    PGPMime,
}

impl PGPScheme {
    /// Returns the string representation of a PGP scheme.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PGPInline => "pgp-inline",
            Self::PGPMime => "pgp-mime",
        }
    }

    /// Returns true if the input string represents a valid PGP scheme
    pub fn valid(other: &str) -> bool {
        matches!(other, "pgp-inline" | "pgp-mime")
    }
}

impl FromStr for PGPScheme {
    type Err = ParsePGPSchemeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "pgp-inline" => Ok(Self::PGPInline),
            "pgp-mime" => Ok(Self::PGPMime),
            _ => Err(ParsePGPSchemeError),
        }
    }
}

/// Preferred mime type to receive an email with.
#[derive(Debug, Default, Clone, Copy, Eq, Hash, PartialEq)]
pub enum EmailMimeType {
    #[default]
    Html,
    Text,
}

impl EmailMimeType {
    /// Returns the string representation of the mime type.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Html => "text/html",
            Self::Text => "text/plain",
        }
    }
}

impl FromStr for EmailMimeType {
    type Err = ParsePGPSchemeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "text/html" => Ok(Self::Html),
            "text/plain" => Ok(Self::Text),
            _ => Err(ParsePGPSchemeError),
        }
    }
}

/// Pinned keys represent public address keys extracted from a contact's v-card.
#[derive(Default, Debug, Clone)]
pub struct PinnedPublicKeys<Pub: PublicKey> {
    /// The imported and extracted public keys form the v-card.
    pub pinned_keys: Vec<Pub>,
    /// Extracted from `x-pm-encrypt` on the v-card email property group.
    pub encrypt_to_pinned: Option<bool>,
    /// Extracted from `x-pm-encrypt-untrusted` on the v-card email property group.
    pub encrypt_to_untrusted: Option<bool>,
    /// Extracted from `x-pm-sign` on the v-card email property group.
    pub sign: Option<bool>,
    /// Extracted from `x-pm-scheme` on the v-card email property group.
    pub scheme: Option<PGPScheme>,
    /// Extracted from `x-pm-mimetype` on the v-card email property group.
    pub mime_type: Option<EmailMimeType>,
    /// Indicates if the pinned keys got extracted from a contact
    /// v-card with a verified signature.
    pub contact_signature_verified: bool,
    /// If a v-card signature got verified, contains the signature's timestamp.
    pub signature_timestamp: Option<UnixTimestamp>,
}

impl<Pub: PublicKey> PinnedPublicKeys<Pub> {
    /// Creates pinned keys with the imported keys using default config values.
    pub fn new(pinned_keys: Vec<Pub>) -> PinnedPublicKeys<Pub> {
        Self {
            pinned_keys,
            encrypt_to_pinned: None,
            encrypt_to_untrusted: None,
            sign: None,
            scheme: None,
            mime_type: None,
            contact_signature_verified: false,
            signature_timestamp: None,
        }
    }

    /// Finds a valid matching encryption key from the list of pinned keys.
    ///
    /// This function iterates through the `pinned_keys` and attempts to find a matching encryption key
    /// that can be used for encryption at the given `encryption_time`.
    ///
    /// # Parameters
    ///
    /// * `api_keys` - A reference to `InboxPublicKeys<Pub>` containing the public keys and recipient type
    ///   to be matched against the pinned keys.
    /// * `encryption_time` - A `UnixTimestamp` representing the time at which the encryption will be performed.
    ///
    /// # Returns
    ///
    /// Returns an `Option<Pub>` containing the first valid matching public key that can be used for encryption,
    /// or `None` if no valid key is found.
    ///
    /// # Conditions for a key to be considered valid:
    ///
    /// 1. If there is matching api key the key flags must be valid.
    /// 2. If the recipient type is `Internal`, there must be a matching public key in `api_keys`.
    /// 3. The key must not be compromised or revoked at the time of encryption.
    /// 4. The key must be able to encrypt at the given `encryption_time`.
    /// 5. The key must not be expired at the time of encryption.
    ///
    /// If any of these conditions are not met, the key is considered invalid and the function will continue
    /// to search for another key.
    pub fn find_valid_matching_encryption_key(
        &self,
        api_keys: &InboxPublicKeys<Pub>,
        encryption_time: UnixTimestamp,
    ) -> Option<Pub> {
        self.pinned_keys.iter().find_map(|pinned_key| {
            let fingerprint = pinned_key.key_fingerprint();
            let matching_public = api_keys
                .public_keys
                .iter()
                .find(|api_key| api_key.public_keys.key_fingerprint() == fingerprint);

            // Check if the key is invalid based on the given conditions
            // TODO: In a more advanced version we might want to signal if something is invalid
            // Maybe, by emitting warnings? 
            let is_invalid = matches!(
                matching_public,
                Some(matching_key) if matching_key.flags.is_compromised() || matching_key.flags.is_obsolete()
            ) || (api_keys.recipient_type == RecipientType::Internal
                && matching_public.is_none())
                || !pinned_key.can_encrypt(encryption_time)
                || pinned_key.is_expired(encryption_time)
                || pinned_key.is_revoked(encryption_time);

            if is_invalid {
                None
            } else {
                Some(pinned_key.clone())
            }
        })
    }
}
