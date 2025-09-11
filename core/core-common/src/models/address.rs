use std::sync::Arc;

use crate::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, InitializationKey,
    LocalAddressId,
};
use crate::{CoreContextError, CoreContextResult};
use proton_core_api::services::proton::{Address as ApiAddress, AddressId, ProtonCore};

use stash::exports::Transaction;
use stash::macros::Model;
use stash::orm::{DbRecord, Model, ModelHooks};
use stash::params;
use stash::rusqlite::params_from_iter;
use stash::stash::{Stash, StashError, StashResult, Tether};

use crate::models::ModelIdExtension;

use super::{InitializationError, InitializationWatcher, InitializedComponent};

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("addresses")]
#[ModelHooks]
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
}

impl ModelHooks for Address {
    fn before_save(&mut self, bond: &Transaction<'_>) -> Result<(), StashError> {
        // WARN: For perfomance reasons this will NOT be called in the initial sync. See `SyncedAddress::store`
        // Any extra logic here should be copied there.
        if let Some(remote_id) = &self.remote_id
            && let Some(existing) = Self::find_by_remote_id_sync(remote_id, bond)?
        {
            self.local_id = existing.local_id;
        }

        Ok(())
    }
}

impl ModelIdExtension for Address {
    type RemoteId = AddressId;

    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
}

impl Address {
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
        InitializedComponent::initialize(
            watcher,
            Self::INIT_KEY,
            &[],
            stash.connection().await?,
            async || Self::sync(api).await,
            |tx, res| {
                res.store(tx)?;
                Ok(())
            },
        )
        .await
    }

    /// Download user addresses. Returns an object that can be stored in DB.
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

    #[must_use]
    pub fn with_signature(mut self, signature: impl Into<String>) -> Self {
        self.signature = signature.into();
        self
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
        }
    }
}

#[must_use]
#[derive(Debug)]
pub struct SyncedAddresses {
    addresses: Vec<Address>,
}

impl SyncedAddresses {
    #[tracing::instrument(skip(tx))]
    pub fn store(self, tx: &Transaction<'_>) -> StashResult<()> {
        let mut query = tx.prepare(Address::INSERT_QUERY)?;
        for address in self.addresses {
            let params = params_from_iter(address.field_values());
            let _rows = query.query(params)?.next()?;
        }
        Ok(())
    }

    #[must_use]
    pub fn inner(self) -> Vec<Address> {
        self.addresses
    }
}
