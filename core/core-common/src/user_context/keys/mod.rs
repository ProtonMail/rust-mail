//! Contains the logic to access PGP keys from the `UserContext`.
mod cache;
mod vcard_crypto;

use bytes::Buf;
use cache::{CachedAddressKey, CachedUserKey};
mod manager;
use crate::{
    ContactError, CoreContextError,
    models::{Contact, ContactCard, ContactEmail},
};
use crate::{CoreContextResult, UserContext};
use ical::{VcardParser, parser::ParserError};
pub use manager::*;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::proton::PrivateEmailRef;
use proton_core_api::{services::proton::AddressId, session::Session};
use proton_crypto_account::{
    contacts::{ContactCardType, DecryptableVerifiableCard},
    errors::CardCryptoError,
    keys::{PinnedPublicKeys, PublicAddressKeys, UnlockedAddressKeys, UnlockedUserKeys},
    proton_crypto::{CryptoError, crypto::PGPProviderSync},
};
use proton_vcard::{VCardError, vcard::VCard};
use stash::stash::RunTransaction;
use stash::{
    orm::Model,
    params,
    stash::{StashError, Tether},
};
use thiserror::Error;
use tracing::{debug, error};

#[allow(clippy::module_name_repetitions)]
type CachedUserKeys = Vec<CachedUserKey>;
#[allow(clippy::module_name_repetitions)]
type CachedAddressKeys = Vec<CachedAddressKey>;

/// Result for key handling  operations.
pub type KeyHandlingResult<T> = Result<T, KeyHandlingError>;

/// An error type that is thrown when loading keys
/// via the [`CryptoKeyManager`].
#[derive(Debug, Error)]
pub enum KeyHandlingError {
    #[error("No user found")]
    NoUser,
    #[error("No user secret found")]
    NoUserSecret,
    #[error("No user keys unlocked but has {0} user keys")]
    UserKeyUnlock(usize),
    #[error("Failed to store user keys in the cache {0}")]
    UserKeyCacheStore(#[from] CryptoError),
    #[error("No address found for id {0}")]
    NoAddress(AddressId),
    #[error("Failed to unlock at least one address key, but the user has {0} address keys")]
    AddressKeyUnlock(usize),
    #[error("Database Error: {0}")]
    DB(#[from] StashError),
    #[error("Problem decrypting and/or verifying the signature of the contact card: {0}")]
    CardDecryptionVerificationError(#[from] CardCryptoError),
    #[error("Failed to validate contact v-card: {0}")]
    VCard(#[from] VCardError),
    #[error("Failed to parse contact v-card: {0}")]
    VCardParse(#[from] ParserError),
    #[error("No contact v-card found in signed data")]
    NoVCard,
}

/// A trait that loads the user secret to unlock the user keys.
#[allow(async_fn_in_trait)]
pub trait LoadKeySecret {
    /// Loads the user secret to unlock the user keys.
    async fn key_secret(&self) -> Option<UserKeySecret>;
}

impl LoadKeySecret for Session {
    async fn key_secret(&self) -> Option<UserKeySecret> {
        self.expose_key_secret().await
    }
}

impl UserContext {
    /// Returns the unlocked user keys of this user.
    ///
    /// First tries to retrieve them from the cache else
    /// it loads and unlocks them from the database.
    pub async fn unlocked_user_keys<P, S>(
        &self,
        pgp: &P,
        conn: &Tether,
        secret_loader: &S,
    ) -> CoreContextResult<UnlockedUserKeys<P>>
    where
        P: PGPProviderSync,
        S: LoadKeySecret,
    {
        self.key_manager
            .user_keys(pgp, conn, secret_loader, &self.user_id)
            .await
    }

    /// Returns the unlocked address keys of this user for the given address.
    ///
    /// Loads the address keys from the database and unlocks them with the user keys.
    pub async fn unlocked_address_keys<P, S>(
        &self,
        pgp: &P,
        conn: &Tether,
        secret_loader: &S,
        address_id: &AddressId,
    ) -> CoreContextResult<UnlockedAddressKeys<P>>
    where
        P: PGPProviderSync,
        S: LoadKeySecret,
    {
        self.key_manager
            .address_keys(pgp, conn, secret_loader, &self.user_id, address_id)
            .await
    }

    /// Loads the public address keys for an email address from the backend API.
    ///
    /// Imports the keys with the PGP provider. In the future, this function will also
    /// verify the keys with key transparency.
    pub async fn public_address_keys<P>(
        &self,
        pgp: &P,
        email: PrivateEmailRef<'_>,
        internal_only: bool,
        fetch_policy: PublicAddressKeyFetchPolicy,
    ) -> CoreContextResult<PublicAddressKeys<<P>::PublicKey>>
    where
        P: PGPProviderSync,
    {
        self.key_manager
            .public_address_keys(pgp, email, internal_only, fetch_policy, self)
            .await
    }

    /// Loads the public address keys pinned to a user's contact, if any.
    ///
    /// Performs the following operations:
    /// - Searches for the contact email matching the input email in the database.
    /// - If no contact matches returns None, else Syncs the full contact with cards
    /// - Extracts the pinned keys from the signed `VCard` if any
    /// - Verifies the signature of the `VCard` with the unlocked user keys
    /// - Returns the pinned keys if any else None
    #[tracing::instrument(skip_all, fields(email=%email))]
    pub async fn public_address_keys_from_contacts<P>(
        &self,
        pgp: &P,
        tx: &mut impl RunTransaction,
        unlocked_user_keys: &UnlockedUserKeys<P>,
        email: PrivateEmailRef<'_>,
        fetch_policy: AddressKeysContactFetchPolicy,
    ) -> CoreContextResult<Option<PinnedPublicKeys<<P>::PublicKey>>>
    where
        P: PGPProviderSync,
    {
        let contact_email =
            ContactEmail::find_first("WHERE email = ?", params![email.to_owned()], tx.tether())
                .await?
                .ok_or(ContactError::CardNotFound(email.to_owned()))?;

        let local_contact_id =
            contact_email
                .local_contact_id
                .ok_or(ContactError::ContactCardRemoteIdNotPresent(
                    email.to_owned(),
                ))?;

        // If a contact exists and has linked vCards, attempt to extract pinned keys from them.
        // vCards should be current if they were synced at least once
        // since they would be updated via update events.
        match fetch_policy {
            AddressKeysContactFetchPolicy::AllowCachedFallback => {
                match Contact::load(local_contact_id, tx.tether()).await {
                    Ok(Some(mut contact)) => {
                        if let Ok(cards) = contact.cards(tx.tether()).await
                            && !cards.is_empty()
                        {
                            debug!(
                                "Use local contact {local_contact_id} for pinned keys extraction"
                            );
                            return Ok(extract_pinned_keys(
                                pgp,
                                unlocked_user_keys,
                                cards,
                                &email,
                            )?);
                        }
                    }
                    Err(e) => {
                        error!("Failed to load contact for pinned keys extraction: {e}");
                    }
                    _ => {}
                }
            }
            AddressKeysContactFetchPolicy::RequireSync => {} // continue
        }

        // On success try to sync the most recent full contact including its v-cards from the BE.
        if let Err(e) = Contact::force_sync_with_card(local_contact_id, self.session(), tx)
            .await
            .inspect_err(|e| error!("Failed to force sync contact: {e}"))
        {
            match e {
                CoreContextError::Api(e) if e.is_network_failure() => match fetch_policy {
                    AddressKeysContactFetchPolicy::RequireSync => return Err(e.into()),
                    AddressKeysContactFetchPolicy::AllowCachedFallback => {} // continue
                },
                e => return Err(e),
            }
        }

        let mut contact = Contact::load(local_contact_id, tx.tether())
            .await?
            .ok_or(ContactError::FullContactNotFound(email.to_owned()))?;

        let cards = contact.cards(tx.tether()).await?;

        Ok(extract_pinned_keys(pgp, unlocked_user_keys, cards, &email)?)
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub enum AddressKeysContactFetchPolicy {
    #[default]
    // Always requires the contacts to be synced from the server, and will error out if
    // the request fails.
    RequireSync,
    // If the request fails due to lack of network, attempt to load existing cached data.
    AllowCachedFallback,
}
impl From<AddressKeysContactFetchPolicy> for PublicAddressKeyFetchPolicy {
    fn from(value: AddressKeysContactFetchPolicy) -> Self {
        match value {
            AddressKeysContactFetchPolicy::RequireSync => Self::RequireSync,
            AddressKeysContactFetchPolicy::AllowCachedFallback => Self::AllowCachedFallback,
        }
    }
}

/// Helper function to extract pinned keys from a contact with cards.
fn extract_pinned_keys<P>(
    pgp: &P,
    unlocked_user_keys: &UnlockedUserKeys<P>,
    cards: &[ContactCard],
    email: &PrivateEmailRef<'_>,
) -> Result<Option<PinnedPublicKeys<<P>::PublicKey>>, KeyHandlingError>
where
    P: PGPProviderSync,
{
    // The pinned key information can be found in the signed v-card.
    let signed_card_opt = cards
        .iter()
        .find(|card| card.card_type == ContactCardType::Signed);

    let Some(signed_card) = signed_card_opt else {
        return Ok(None);
    };

    let mut verification_keys = Vec::with_capacity(unlocked_user_keys.len());

    unlocked_user_keys
        .iter()
        .for_each(|uuk| verification_keys.push(uuk.public_key.clone()));

    // Verify the signature of the v-card.
    let card_data =
        signed_card.decrypt_and_verify_sync(pgp, &pgp.empty_private_keys(), &verification_keys)?;

    // Parse the v-card contact, there should be exactly one v-card
    let vcard_contact = VcardParser::new(card_data.reader())
        .next()
        .ok_or(KeyHandlingError::NoVCard)??;

    let vcard = VCard::try_from(vcard_contact)?;

    Ok(vcard_crypto::pinned_keys_for_mail(&vcard, pgp, email))
}
