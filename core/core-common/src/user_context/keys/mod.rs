//! Contains the logic to access PGP keys from the `UserContext`.
mod cache;
mod vcard_crypto;

use bytes::Buf;
use cache::{CachedAddressKey, CachedUserKey};
mod manager;
use crate::{
    ContactError,
    models::{Contact, ContactEmail},
};
use crate::{CoreContextResult, UserContext};
use ical::{VcardParser, parser::ParserError};
pub use manager::*;
use proton_core_api::{auth::UserKeySecret, session::CoreSession};
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
use tracing::{Level, debug};

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
    ///
    /// # Parameters
    ///
    /// * `pgp`           - The pgp provider instance from `proton_crypto`.
    /// * `conn`          - The database connection to load the keys from database.
    /// * `secret_loader` - The struct providing the access to the secret needed to unlock the user keys
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
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
    ///
    /// # Parameters
    ///
    /// * `pgp`            - The pgp provider instance from `proton_crypto`.
    /// * `conn`           - The database connection to load the keys from database.
    /// * `secret_load_fn` - The struct providing the access to the secret needed to unlock the user keys
    /// * `address_id`     - The ID of the address key
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
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
    ///
    /// # Parameters
    ///
    /// * `pgp` - The pgp provider instance from `proton_crypto`.
    /// * `email` - The email address the public address keys are being requested for.
    /// * `internal_only` - A flag used to indicate if the keys requested are internal-only, i.e. keys for a Proton user.
    ///
    /// # Errors
    /// Returns a wrapped [`KeyHandlingError`] if the operation fails.
    pub async fn public_address_keys<P>(
        &self,
        pgp: &P,
        email: &str,
        internal_only: bool,
    ) -> CoreContextResult<PublicAddressKeys<<P>::PublicKey>>
    where
        P: PGPProviderSync,
    {
        self.key_manager
            .public_address_keys(pgp, email, internal_only, self)
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
    ///
    /// Parameters
    ///
    /// * `pgp`                - The pgp provider instance from `proton_crypto`.
    /// * `db_interface`       - The database interface to query from.
    /// * `unlocked_user_keys` - Unlocked keys for the current user, these are used to decrypt and verify the contact cards.
    /// * `email`              - the email address that keys are being sought for.
    ///
    /// # Errors
    /// Returns an error on a database or sync failure.
    /// - A DB/IO error if syncing the contact or accessing the contacts fails.
    /// - A wrapped [`KeyHandlingError`] if `VCard` parsing or signature verification fails.
    #[tracing::instrument(level = Level::DEBUG, skip(self, pgp, tx, unlocked_user_keys))]
    pub async fn public_address_keys_from_contacts<P>(
        &self,
        pgp: &P,
        tx: &mut impl RunTransaction,
        unlocked_user_keys: &UnlockedUserKeys<P>,
        email: &str,
    ) -> CoreContextResult<Option<PinnedPublicKeys<<P>::PublicKey>>>
    where
        P: PGPProviderSync,
    {
        // First, we try to load an contact emails that matches the email.
        debug!("Try to load the contact email for {email} from the db");

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

        // On success try to sync the most recent full contact including its v-cards from the BE.
        Contact::force_sync_with_card(local_contact_id, self.session().api(), tx).await?;

        let mut contact = Contact::load(local_contact_id, tx.tether())
            .await?
            .ok_or(ContactError::FullContactNotFound(email.to_owned()))?;

        debug!("Full contact with cards found");

        Ok(extract_pinned_keys(pgp, tx.tether(), unlocked_user_keys, &mut contact, email).await?)
    }
}

/// Helper function to extract pinned keys from a contact with cards.
async fn extract_pinned_keys<P>(
    pgp: &P,
    db: &Tether,
    unlocked_user_keys: &UnlockedUserKeys<P>,
    full_contact: &mut Contact,
    email: &str,
) -> Result<Option<PinnedPublicKeys<<P>::PublicKey>>, KeyHandlingError>
where
    P: PGPProviderSync,
{
    // The pinned key information can be found in the signed v-card.
    let signed_card_opt = full_contact
        .cards(db)
        .await?
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
