use std::sync::Arc;

use crate::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, InitializationKey,
    LocalAddressId,
};
use crate::{CoreContextError, CoreContextResult};
use proton_core_api::services::proton::Address as ApiAddress;
use proton_core_api::services::proton::AddressId;
use proton_core_api::services::proton::ProtonCore;

use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::StashError;
use stash::stash::Tether;
use stash::stash::{Bond, Stash};

use crate::models::ModelIdExtension;

use super::{InitializationError, InitializationWatcher, InitializedComponent};

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("addresses")]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalAddressId>,

    #[DbField]
    pub remote_id: Option<AddressId>,

    #[DbField]
    pub address_type: AddressType,

    #[DbField]
    pub catch_all: bool,

    #[DbField]
    pub display_name: String,

    #[DbField]
    pub display_order: u32,

    #[DbField]
    pub domain_id: Option<String>,

    #[DbField]
    pub email: String,

    #[DbField]
    pub keys: AddressKeys,

    #[DbField]
    pub proton_mx: bool,

    #[DbField]
    pub receive: bool,

    #[DbField]
    pub send: bool,

    #[DbField]
    pub signature: String,

    #[DbField]
    pub signed_key_list: AddressSignedKeyList,

    #[DbField]
    pub status: AddressStatus,

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl ModelIdExtension for Address {
    type RemoteId = AddressId;

    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
}

impl Address {
    /// Save an address to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_remote_id(remote_id, bond).await? {
                self.row_id = existing.row_id;
                self.local_id = existing.local_id;
            }
        }

        <Self as Model>::save(self, bond).await
    }

    /// Key used to distinguish between components in the initialization.
    /// It is a string, not an enum for making it open for additional changes from different BU.
    ///
    pub const INIT_KEY: InitializationKey = InitializationKey::new("addresses");
    /// It initializes addresses by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize<API>(
        watcher: Arc<InitializationWatcher>,
        api: &API,
        stash: &Stash,
    ) -> Result<(), InitializationError<CoreContextError>>
    where
        API: ProtonCore,
    {
        InitializedComponent::initialize::<CoreContextError, SyncedAddresses>(
            watcher,
            Self::INIT_KEY,
            &[],
            stash.connection(),
            async || Self::sync(api).await,
            async |tx, res| {
                res.store(tx).await?;
                Ok(())
            },
        )
        .await
    }

    /// Download user addresses. Returns an object that can be stored in DB.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use to download the addresses.
    ///
    /// # Errors
    ///
    /// TODO: Document the errors.
    ///
    pub async fn sync(api: &impl ProtonCore) -> CoreContextResult<SyncedAddresses> {
        let addresses = api
            .get_addresses()
            .await?
            .addresses
            .into_iter()
            .map(Address::from)
            .collect();

        Ok(SyncedAddresses { addresses })
    }

    /// Loads the address for the given e-mail from the database if any.
    ///
    /// Returns [`None`] if no address with the given email is found.
    ///
    /// # Parameters
    ///
    /// * `email`     - The e-mail address to search for.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the records.
    /// # Errors
    ///
    /// Returns a [`StashError`] if the database access fails.
    ///
    pub async fn by_email(email: &str, tether: &Tether) -> Result<Option<Address>, StashError> {
        Self::find_first("WHERE email = ?", params![email.to_owned()], tether).await
    }

    pub async fn all_send_enabled(tether: &Tether) -> Result<Vec<Address>, StashError> {
        Address::find(
            "WHERE send=? AND status = ? ORDER BY display_order ASC",
            params![true, AddressStatus::Enabled],
            tether,
        )
        .await
    }
}

impl From<ApiAddress> for Address {
    fn from(value: ApiAddress) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id),
            address_type: value.address_type.into(),
            catch_all: value.catch_all,
            display_name: value.display_name,
            display_order: value.order,
            domain_id: value.domain_id,
            email: value.email,
            keys: value.keys.into(),
            proton_mx: value.proton_mx,
            receive: value.receive,
            send: value.send,
            signature: value.signature,
            signed_key_list: value.signed_key_list.into(),
            status: value.status.into(),
            row_id: None,
        }
    }
}

/// This is a manual implementation of `Address::sync` async closure.
///
/// We keep it as it is until Rust allows us to use `impl Trait` in generics etc.
#[must_use]
#[derive(Debug)]
pub struct SyncedAddresses {
    addresses: Vec<Address>,
}

impl SyncedAddresses {
    /// Consume this manual closure by storing data in the Database.
    ///
    #[tracing::instrument(skip(tx))]
    pub async fn store(self, tx: &Bond<'_>) -> CoreContextResult<()> {
        for mut address in self.addresses {
            address.save(tx).await?;
        }
        Ok(())
    }

    #[must_use]
    pub fn inner(self) -> Vec<Address> {
        self.addresses
    }
}
