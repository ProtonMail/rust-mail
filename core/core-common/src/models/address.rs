use crate::CoreContextResult;
use crate::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, LocalAddressId,
};
use proton_api_core::services::proton::Address as ApiAddress;
use proton_api_core::services::proton::AddressId;
use proton_api_core::services::proton::Proton;
use proton_api_core::services::proton::ProtonCore;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::Bond;
use stash::stash::Tether;
use stash::stash::{Stash, StashError};

use crate::models::ModelIdExtension;

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("addresses")]
#[allow(clippy::struct_excessive_bools)]
pub struct Address {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalAddressId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<AddressId>,

    /// TODO: Document this field.
    #[DbField]
    pub address_type: AddressType,

    /// TODO: Document this field.
    #[DbField]
    pub catch_all: bool,

    /// TODO: Document this field.
    #[DbField]
    pub display_name: String,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u32,

    /// TODO: Document this field.
    #[DbField]
    pub domain_id: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub email: String,

    /// TODO: Document this field.
    #[DbField]
    pub keys: AddressKeys,

    /// TODO: Document this field.
    #[DbField]
    pub proton_mx: bool,

    /// TODO: Document this field.
    #[DbField]
    pub receive: bool,

    /// TODO: Document this field.
    #[DbField]
    pub send: bool,

    /// TODO: Document this field.
    #[DbField]
    pub signature: String,

    /// TODO: Document this field.
    #[DbField]
    pub signed_key_list: AddressSignedKeyList,

    /// TODO: Document this field.
    #[DbField]
    pub status: AddressStatus,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
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

    /// Download and store user addresses into the database
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use to download the addresses.
    /// * `stash` - The database instance to store the addresses.
    ///
    /// # Errors
    ///
    /// TODO: Document the errors.
    ///
    pub async fn sync(api: &Proton, stash: &Stash) -> CoreContextResult<()> {
        let addresses = api
            .get_addresses()
            .await?
            .addresses
            .into_iter()
            .map(Address::from);

        let mut conn = stash.connection();
        let tx = conn.transaction().await?;
        for mut address in addresses {
            address.save(&tx).await?;
        }
        tx.commit().await?;

        Ok(())
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
