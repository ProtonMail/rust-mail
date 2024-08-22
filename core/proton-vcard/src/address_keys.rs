use crate::{parameters::preference::Preference, properties::key::KeyValue, vcard::VCard};
use base64::{prelude::BASE64_STANDARD as BASE_64, DecodeError, Engine as _};

use proton_crypto_account::{
    keys::{PGPScheme, PinnedPublicKeys},
    proton_crypto::{
        crypto::{DataEncoding, PGPProviderSync, PublicKey},
        CryptoError,
    },
};
use thiserror::Error;
use tracing::error;

#[derive(Debug, Error)]
pub enum PGPKeyImportError {
    #[error("no key data in card to import")]
    NoData,
    #[error("error decoding Base64 data: {0}")]
    Base64Decode(#[from] DecodeError),
    #[error("error importing PGP key: {0}")]
    PGPError(#[from] CryptoError),
}

pub const X_PM_ENCRYPT: &str = "X-PM-ENCRYPT";
pub const X_PM_ENCRYPT_UNTRUSTED: &str = "X-PM-ENCRYPT-UNTRUSTED";
pub const X_PM_SCHEME: &str = "X-PM-SCHEME";
pub const X_PM_SIGN: &str = "X-PM-SIGN";

impl VCard {
    /// Returns all pinned keys for this v-card contact matching the provided email address.
    ///
    /// The email comparison ignores case-sensitivity.
    ///
    /// If no pinned keys are detected, the method returns [`None`].
    ///
    /// # Parameters
    ///
    /// * `pgp_provider` - The pgp provider instance from `proton_crypto`.
    /// * `email` - the email address that keys are being searched for in the `VCard`.
    pub fn pinned_keys_for_mail<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        email: &str,
    ) -> Option<PinnedPublicKeys<<Provider>::PublicKey>> {
        let group = self.property_group_for_email(email)?;
        let mut pinned_keys = self.pinned_keys_for_group(pgp_provider, &group)?;
        self.update_pinned_keys_with_extended_preferences(&group, &mut pinned_keys);
        Some(pinned_keys)
    }

    /// Find the property group for the provided mail.
    fn property_group_for_email(&self, target_email: &str) -> Option<String> {
        self.get_all_email().into_iter().find_map(|(_, email)| {
            if email.value.value == target_email {
                return email.group;
            }
            None
        })
    }

    /// Collect all the keys for the `selected_group` and return them in order of preference.
    /// A lower value for preference indicates a higher priority for that key
    fn pinned_keys_for_group<Provider: PGPProviderSync>(
        &self,
        pgp_provider: &Provider,
        selected_group: &str,
    ) -> Option<PinnedPublicKeys<<Provider>::PublicKey>> {
        let mut preference_keys = self
            .get_all_key()
            .into_iter()
            .filter_map(|(_, key)| {
                let group_name = key.group?;
                if group_name == selected_group {
                    let key_data = match key.value {
                        KeyValue::Text(data) => data.value,
                        KeyValue::Uri(uri) => uri.0.to_string(),
                    };

                    let pref = unwrap_preference(key.preference);
                    let public_key_res = parse_and_import_pgp_key(pgp_provider, &key_data);

                    match public_key_res {
                        Err(e) => {
                            error!("error parsing and importing pgp key with error: {:?}", e);
                            return None;
                        }
                        Ok(public_key) => return Some((pref, public_key?)),
                    }
                }
                None
            })
            .collect::<Vec<_>>();

        if preference_keys.is_empty() {
            return None;
        }

        preference_keys.sort_by(|a, b| a.0.cmp(&b.0));
        let mut pinned_keys = Vec::with_capacity(preference_keys.len());
        pinned_keys.extend(preference_keys.into_iter().map(|val| val.1));
        Some(PinnedPublicKeys::new(pinned_keys))
    }

    /// Updates the pinned public key preferences in `pinned_keys`
    /// based on the preferences found in the matching selected group.
    fn update_pinned_keys_with_extended_preferences<Pub: PublicKey>(
        &self,
        selected_group: &str,
        pinned_keys: &mut PinnedPublicKeys<Pub>,
    ) {
        self.get_all_xtended()
            .into_iter()
            .filter_map(|(_, property)| {
                if property.group.as_ref()? == selected_group {
                    Some(property)
                } else {
                    None
                }
            })
            .for_each(
                |extended_property| match extended_property.name.0.as_str() {
                    X_PM_ENCRYPT => {
                        pinned_keys.encrypt_to_pinned =
                            parse_bool(extended_property.value.as_deref());
                    }
                    X_PM_ENCRYPT_UNTRUSTED => {
                        pinned_keys.encrypt_to_untrusted =
                            parse_bool(extended_property.value.as_deref());
                    }
                    X_PM_SCHEME => {
                        pinned_keys.scheme = parse_pgp_scheme(extended_property.value.as_deref());
                    }
                    X_PM_SIGN => {
                        pinned_keys.sign = parse_bool(extended_property.value.as_deref());
                    }
                    _ => (),
                },
            );
    }
}

fn unwrap_preference(preference: Option<Preference>) -> Preference {
    if let Some(unwrapped) = preference {
        if unwrapped.is_valid_value() {
            unwrapped
        } else {
            Preference::less_than_lowest()
        }
    } else {
        Preference::less_than_lowest()
    }
}

/// Helper function to parse a [`bool`] from a property value.
///
/// Returns [`None`] if parsing fails.
fn parse_bool(value: Option<&str>) -> Option<bool> {
    value
        .as_ref()
        .and_then(|str| str.to_lowercase().parse::<bool>().ok())
}

/// Helper function to parse a [`PGPScheme`] from a property value.
///
/// Returns [`None`] if parsing fails.
fn parse_pgp_scheme(value: Option<&str>) -> Option<PGPScheme> {
    value.as_ref().and_then(|str| str.parse::<PGPScheme>().ok())
}

/// Helper function to parse and import a PGP key from a property value.
///
/// Returns [`None`] if parsing or input fails.
fn parse_and_import_pgp_key<Provider: PGPProviderSync>(
    pgp_provider: &Provider,
    value: &str,
) -> Result<Option<<Provider>::PublicKey>, PGPKeyImportError> {
    let base64_encoded_key = value.split(',').last().ok_or(PGPKeyImportError::NoData)?;
    let binary_key = BASE_64.decode(base64_encoded_key)?;
    Ok(Some(
        pgp_provider.public_key_import(binary_key, DataEncoding::Bytes)?,
    ))
}
