use std::collections::HashSet;

use proton_crypto_account::{
    keys::{DecryptedAddressKey, PinnedPublicKeys, PublicAddressKeys},
    proton_crypto::{
        crypto::{AsPublicKeyRef, OpenPGPFingerprint, PrivateKey, PublicKey},
        keytransparency::{KT_UNVERIFIED, KT_VERIFIED, KTVerificationResult},
    },
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyOwnership {
    /// The public keys are extracted from self owned keys.
    SelfOwn,
    /// The public keys are from other users.
    Other,
}

/// A type that stores public keys to verify signatures and relevant
/// key information to display.
#[derive(Debug)]
pub struct InboxVerificationPreferences<Pub: PublicKey> {
    /// Where did the keys originated from.
    pub ownership: KeyOwnership,
    /// Pinned public keys.
    pub pinned_keys: Vec<Pub>,
    /// API public keys.
    pub api_keys: Vec<Pub>,
    /// Fingerprints of keys marked as compromised.
    pub compromised_fingerprints: HashSet<OpenPGPFingerprint>,
    /// Key transparency verification result.
    pub key_transparency_verification: KTVerificationResult,
}

impl<Pub: PublicKey> Default for InboxVerificationPreferences<Pub> {
    fn default() -> Self {
        Self {
            ownership: KeyOwnership::Other,
            pinned_keys: Vec::default(),
            api_keys: Vec::default(),
            compromised_fingerprints: HashSet::default(),
            key_transparency_verification: KT_UNVERIFIED,
        }
    }
}

impl<Pub: PublicKey> InboxVerificationPreferences<Pub> {
    /// Selects the valid signature verification keys from the unlocked user keys of the logged-in user.
    pub fn from_unlocked_address_keys<Priv: PrivateKey>(
        address_keys: &[DecryptedAddressKey<Priv, Pub>],
    ) -> InboxVerificationPreferences<Pub> {
        let mut compromised_fingerprints = HashSet::new();
        let active_address_keys = address_keys
            .iter()
            .filter(|key| {
                if key.flags.is_compromised() {
                    compromised_fingerprints.insert(key.as_public_key().key_fingerprint());
                    return false;
                }
                true
            })
            .map(|address_key| address_key.as_public_key().clone())
            .collect::<Vec<_>>();
        InboxVerificationPreferences {
            ownership: KeyOwnership::SelfOwn,
            pinned_keys: Vec::default(),
            api_keys: active_address_keys,
            compromised_fingerprints,
            key_transparency_verification: KT_VERIFIED,
        }
    }

    /// Selects the valid signature verification keys based on the retrieved keys from another user.
    ///
    /// Selects the public keys for signature verification based on the public keys fetched from the API
    /// and the public keys found in the associated contact.
    #[must_use]
    pub fn from_public_keys(
        api_keys: PublicAddressKeys<Pub>,
        vcard_keys: Option<PinnedPublicKeys<Pub>>,
    ) -> InboxVerificationPreferences<Pub> {
        let inbox_keys = api_keys.into_inbox_keys(true);
        // Filter the inbox keys to be non-compromised and collect fingerprints for the compromised ones.
        let mut compromised_fingerprints = HashSet::new();
        let inbox_keys_active = inbox_keys
            .public_keys
            .into_iter()
            .filter(|public_key| {
                if public_key.flags.is_compromised() {
                    compromised_fingerprints.insert(public_key.as_public_key().key_fingerprint());
                    return false;
                }
                true
            })
            .map(|public_key| public_key.public_keys)
            .collect::<Vec<_>>();
        // Filter the pinned keys to not be flagged as compromised via the API.
        let pinned_keys_active = if let Some(keys) = vcard_keys {
            keys.pinned_keys
                .into_iter()
                .filter(|public_key| {
                    !compromised_fingerprints
                        .contains(&public_key.as_public_key().key_fingerprint())
                })
                .collect::<Vec<_>>()
        } else {
            Vec::default()
        };
        InboxVerificationPreferences {
            ownership: KeyOwnership::Other,
            pinned_keys: pinned_keys_active,
            api_keys: inbox_keys_active,
            compromised_fingerprints,
            key_transparency_verification: inbox_keys.key_transparency_verification,
        }
    }

    /// Returns the a reference to the signature verification keys.
    ///
    /// The keys be the input input to the respective signature verification function.
    /// Pinned keys extracted from contacts are preferred over keys from the API.
    #[must_use]
    pub fn signature_verification_keys(&self) -> &[Pub] {
        if self.uses_pinned_keys() {
            return &self.pinned_keys;
        }
        &self.api_keys
    }

    /// Indicates whether contact pinned keys are used by these preferences.
    #[must_use]
    pub fn uses_pinned_keys(&self) -> bool {
        !self.pinned_keys.is_empty()
    }

    /// Checks whether this `OpenPGP` key fingerprint belongs to a key marked as compromised.
    ///
    /// This can be helpful to check whether as signature was created by a key marked as compromised.
    #[must_use]
    pub fn is_compromised(&self, fingerprint: &OpenPGPFingerprint) -> bool {
        self.compromised_fingerprints.contains(fingerprint)
    }

    /// Are the keys extract from self owned keys.
    #[must_use]
    pub fn self_owned_keys(&self) -> bool {
        matches!(self.ownership, KeyOwnership::SelfOwn)
    }
}
