use base64::{DecodeError, Engine as _, prelude::BASE64_STANDARD as BASE_64};
use itertools::Itertools as _;
use proton_vcard::{parameters::preference::Preference, vcard::VCard};

use proton_crypto_account::{
    keys::{PGPScheme, PinnedPublicKeys},
    proton_crypto::{
        CryptoError,
        crypto::{DataEncoding, PGPProviderSync, PublicKey},
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

/// Returns all pinned keys for this v-card contact matching the provided email address.
///
/// The email comparison ignores case-sensitivity.
///
/// If no crypto information for this email is found in the vcard, the method returns [`None`].
///
/// # Parameters
///
/// * `pgp_provider` - The pgp provider instance from `proton_crypto`.
/// * `email` - the email address that keys are being searched for in the `VCard`.
pub fn pinned_keys_for_mail<Provider: PGPProviderSync>(
    vcard: &VCard,
    pgp_provider: &Provider,
    email: &str,
) -> Option<PinnedPublicKeys<<Provider>::PublicKey>> {
    let group = vcard
        .get_all_email()
        .into_iter()
        .find(|(_, email2)| email2.value.value == email)?
        .1
        .group?;

    let mut pinned_keys = pinned_keys_for_group(vcard, pgp_provider, &group);
    update_pinned_keys_with_extended_preferences(vcard, &group, &mut pinned_keys);
    Some(pinned_keys)
}

/// Collect all the keys for the `selected_group` and return them in order of preference.
/// A lower value for preference indicates a higher priority for that key
fn pinned_keys_for_group<Provider: PGPProviderSync>(
    vcard: &VCard,
    pgp_provider: &Provider,
    selected_group: &str,
) -> PinnedPublicKeys<<Provider>::PublicKey> {
    let mut preference_keys = vcard
        .get_all_key()
        .into_iter()
        .filter_map(|(_, key)| {
            let group_name = key.group?;
            if group_name == selected_group {
                let key_data = key.value.to_string();

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
        .collect_vec();

    preference_keys.sort_by(|a, b| a.0.cmp(&b.0));
    let mut pinned_keys = Vec::with_capacity(preference_keys.len());
    pinned_keys.extend(preference_keys.into_iter().map(|val| val.1));
    PinnedPublicKeys::new(pinned_keys)
}

/// Updates the pinned public key preferences in `pinned_keys`
/// based on the preferences found in the matching selected group.
fn update_pinned_keys_with_extended_preferences<Pub: PublicKey>(
    vcard: &VCard,
    selected_group: &str,
    pinned_keys: &mut PinnedPublicKeys<Pub>,
) {
    vcard
        .get_all_xtended()
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
                    pinned_keys.encrypt_to_pinned = parse_bool(extended_property.value.as_deref());
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

/// Helper function to parse a  from a property value.
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
    let base64_encoded_key = value
        .split(',')
        .next_back()
        .ok_or(PGPKeyImportError::NoData)?;
    let binary_key = BASE_64.decode(base64_encoded_key)?;
    Ok(Some(
        pgp_provider.public_key_import(binary_key, DataEncoding::Bytes)?,
    ))
}
