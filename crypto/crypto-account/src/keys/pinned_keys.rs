use std::str::FromStr;

use proton_crypto::crypto::{PublicKey, UnixTimestamp};

use super::InboxPublicKeys;
use std::collections::HashMap;

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

    /// Finds a valid encryption key from the list of pinned keys given the keys from the API.
    ///
    /// Selects the key according the following steps:
    ///  - The client should encrypt the email with the first valid pinned key (the keys in the vCard should be ordered according to their PREF property;
    ///    if that has not been specified they are taken in the order in which they are written in the vCard)
    ///    whose fingerprint matches the fingerprint of one of the keys served by the API.
    ///
    /// - There are pinned keys, but none of the fingerprints of the pinned keys matches the fingerprint of one of the keys served by the API.
    ///   In this case the client should force the user (via a modal) to trust one of the keys served by the API before sending any email.
    ///
    /// - If there are no pinned keys, then the client should encrypt with the first valid key served by the API.
    ///
    /// - If there are no api keys, but pinned keys are present, the first valid pinned key is returned.
    ///
    /// # Returns
    ///
    /// Returns `None` if no key has been found, `KeyTrust::PromptUserToTrust(key)` if an API key is selected but there are pinned keys,
    /// or `KeyTrust::Trusted(key)` if a pinned key is selected.
    ///
    /// # Parameters
    ///
    /// * `api_keys` - A reference to `InboxPublicKeys<Pub>` containing the public keys and recipient type
    ///   to be matched against the pinned keys.
    /// * `encryption_time` - A `UnixTimestamp` representing the time at which the encryption will be performed.
    ///
    pub fn select_encryption_key(
        &self,
        api_keys: &InboxPublicKeys<Pub>,
        encryption_time: UnixTimestamp,
    ) -> Option<KeyTrust<Pub>> {
        let fingerprint_map: HashMap<_, _> = api_keys
            .public_keys
            .iter()
            .map(|api_key| (api_key.public_keys.key_fingerprint(), api_key))
            .collect();

        // The client should encrypt the email with the first pinned key whose fingerprint matches
        // the fingerprint of one of the keys served by the API.
        let selected_key_matching_opt = self.pinned_keys.iter().find_map(|pinned_key| {
            let matching_key = fingerprint_map.get(&pinned_key.key_fingerprint())?;
            let is_invalid = matching_key.flags.is_compromised()
                || matching_key.flags.is_obsolete()
                || !pinned_key.can_encrypt(encryption_time)
                || pinned_key.is_expired(encryption_time)
                || pinned_key.is_revoked(encryption_time);
            if is_invalid {
                None
            } else {
                Some(pinned_key.clone())
            }
        });

        if let Some(selected_key_matching) = selected_key_matching_opt {
            return Some(KeyTrust::Trusted(selected_key_matching));
        }

        // If there are pinned keys, but none of the fingerprints of the pinned keys matches the fingerprint
        // of one of the keys served by the API.
        // In this case the client should force the user (via a modal) to trust one of the keys served by the API before sending any email.
        if let Some(first_api_key) = api_keys.public_keys.first() {
            Some(KeyTrust::PromptUserToTrust(
                first_api_key.public_keys.clone(),
            ))
        } else {
            self.pinned_keys
                .iter()
                .find_map(|pinned_key| {
                    if fingerprint_map.contains_key(&pinned_key.key_fingerprint()) {
                        return None;
                    };
                    if !pinned_key.can_encrypt(encryption_time)
                        || pinned_key.is_expired(encryption_time)
                        || pinned_key.is_revoked(encryption_time)
                    {
                        None
                    } else {
                        Some(pinned_key.clone())
                    }
                })
                .map(KeyTrust::Trusted)
        }
    }
}

/// Type that indicates if a public key can be trusted
/// or should be trusted by the user.
#[derive(Debug, Clone)]
pub enum KeyTrust<Pub: PublicKey> {
    Trusted(Pub),
    PromptUserToTrust(Pub),
}
