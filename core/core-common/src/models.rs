//! Models for the Proton Core common library.
//!
//! This module contains the models used by the Proton Core common library.
//! Models are data structures that can be saved in the database, and are used
//! to represent usable persistent data throughout the application. They are
//! distinctly different from any comparative structures used when interfacing
//! with the Proton API, which are used to represent data in transit only.
//!
//! Notably, the types in this module need to have [`Model`] applied, as they
//! should represent a record in a database table. All of their fields need to
//! be convertible to and from database-compatible format using [`ToSql`](stash::exports::ToSql)
//! and [`FromSql`](stash::exports::FromSql). They do not generally need to be
//! serializable or deserializable, as they are not used for network
//! communication or any other interchange purpose as a general requirement, and
//! so implementation of [`Serialize`](serde::Serialize) and [`Deserialize`](serde::Deserialize)
//! is not necessary and may be a sign of a mistake. The exception here is for
//! child types, used by the models, for which these [`serde`] conversions are
//! desirable to lean on in order to provide conversion to and from SQL types,
//! for instance using [`sql_using_serde`](stash::utils::sql_using_serde), as a
//! convenience mechanism. This is notably useful when wanting to store types as
//! JSON in a database field, for instance. However, child types should be
//! placed into the [`datatypes`](crate::datatypes) module, with only
//! first-order models being placed into this module.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton API
//! types to the internal types are provided, but not vice versa unless there is
//! a specific need.
//!

#[cfg(test)]
#[path = "tests/models.rs"]
mod tests;

pub mod contact;
pub mod contact_card;
pub mod contact_email;
pub mod device;
pub mod labels;

pub use self::contact::*;
pub use self::contact_card::*;
pub use self::contact_email::*;
pub use self::device::*;
pub use self::labels::*;

use crate::CoreContextResult;
use crate::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, DateFormat, Density, Email,
    Flags, HighSecurity, LocalAddressId, LocalIdMarker, LogAuth, Password, Phone, ProductUsedSpace,
    Referral, SettingsFlags, TimeFormat, TwoFa, UserKeys, UserMnemonicStatus, UserType, WeekStart,
};
use indoc::formatdoc;
use itertools::Itertools as _;
use proton_api_core::services::proton::Proton;
use proton_api_core::services::proton::ProtonCore;
use proton_api_core::services::proton::{
    Address as ApiAddress, User as ApiUser, UserSettings as ApiUserSettings,
};
use proton_api_core::services::proton::{AddressId, ProtonIdMarker, UserId};
use stash::exports::{SqliteError, ToSql};
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::Bond;
use stash::stash::Tether;
use stash::stash::{Stash, StashError};

#[allow(async_fn_in_trait)]
pub trait ModelExtension: Model {
    /// Finds all records in the database.
    ///
    /// This is a convenience method for when all records need to be loaded
    /// without any criteria. This happens remarkably often, and centralises the
    /// functionality, plus makes the intent clear.
    ///
    /// # Parameters
    ///
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///   to use for finding the records.
    /// * `queue`       - An optional queue to send changes to. If this is
    ///   provided, the function will listen for changes to the
    ///   result set and send them to the queue. This is useful
    ///   for live updates.
    ///
    /// # Errors
    ///
    /// See [`Model::find()`].
    ///
    /// # See also
    ///
    /// * [`find()`](Model::find())
    ///
    #[must_use]
    async fn all(tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find(String::new(), vec![], tether).await
    }

    /// Finds a record by its ID.
    ///
    /// The [`load()`](Model::load()) method is so-called to be the counterpart
    /// to [`save()`](Model::save()), but could equally be called `find_by_id()`
    /// under familiar naming conventions. The reason that was not used is that
    /// we have multiple ID types, and so "load" is a more generic term that is
    /// closely associated with local representations.
    ///
    /// However, there is a need to find records by their remote IDs, and indeed
    /// a need to *generically* find records regardless of ID type. Hence this
    /// method is provided to help with that. It is a convenience method that
    /// calls [`load()`](Id::load()) on the ID type itself.
    ///
    /// It does very little, and exists to formalise the interface for carrying
    /// out this process, for uniformity and centralisation of this common
    /// operation.
    ///
    /// # Parameters
    ///
    /// * `id`        - The ID of the record to find.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the record.
    ///
    /// # Errors
    ///
    /// See [`Model::find_first()`].
    ///
    /// # See also
    ///
    /// * [`find_first()`](Model::find_first())
    /// * [`load()`](Model::load())
    ///
    async fn find_by_id(id: Self::IdType, tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::find_first(
            format!("WHERE {} =?", Self::id_field_name()),
            params![id],
            tether,
        )
        .await
    }

    /// Finds a records by its IDs.
    /// Work with `local_id` field or `remote_id` field.
    ///
    /// # Parameters
    ///
    /// * `ids`         - The IDs of the records to find
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the record.
    ///
    /// # Errors
    ///
    /// See [`Model::find_first()`].
    ///
    /// # See also
    ///
    /// * [`find_first()`](Model::find_first())
    /// * [`load()`](Model::load())
    /// * [`load_by_id()`](ModelExtension::load_by_id())
    ///
    async fn find_by_ids(
        ids: impl IntoIterator<Item = Self::IdType>,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let mut ids = ids.into_iter().peekable();
        let field_name = if ids.peek().is_some() {
            // We make the assumption that all ids are the same AgnosticId variant
            Self::id_field_name()
        } else {
            return Ok(vec![]);
        };
        #[allow(trivial_casts)]
        let parameters = ids
            .map(|i| Box::new(i) as Box<dyn ToSql + Send>)
            .collect_vec();
        let placeholders = stash::utils::placeholders(parameters.len());

        let query = format!("WHERE {field_name} IN ({placeholders})");
        Self::find(query, parameters, tether).await
    }

    /// Saves the model by value, returning the updated model.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    async fn with_save(mut self, bond: &Bond<'_>) -> Result<Self, StashError> {
        self.save(bond).await?;
        Ok(self)
    }

    /// Deletes a record by its ID.
    ///
    /// This method is a convenience method for deleting a record by its primary id.
    ///
    /// # Returns
    ///
    /// Returns the number of rows deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to delete the item from the database.
    async fn delete_by_id(id: Self::IdType, bond: &Bond<'_>) -> Result<usize, StashError> {
        let table = Self::table_name();
        let query = format!("DELETE FROM {table} WHERE {} = ?", Self::id_field_name(),);

        bond.execute(query, params![id]).await
    }

    /// Finds record IDs matching given criteria.
    ///
    /// This method is the counterpart to [`find()`](Model::find()), but where
    /// only the local IDs are needed. This saves having to load the entire
    /// model data in order to get the IDs. It operates in the same way as
    /// [`find()`](Model::find()). except it does not support live queries.
    ///
    /// # WARNING
    ///
    /// This method will **ONLY** work with models that have a `local_id` field.
    /// If the model does not follow this convention, use a manual approach.
    ///
    /// # Parameters
    ///
    /// * `query_logic` - The query logic to use for finding the records. This
    ///   should be a string that represents the conditions,
    ///   ordering, offset, and limit for the query, as may be
    ///   required. It can be empty. Note that each part of the
    ///   logic is optional — so if conditions are passed, for
    ///   instance, the `WHERE` keyword needs to be included.
    /// * `params`      - The parameters to use in the query. These should be in
    ///   the order they are expected in the query logic, and
    ///   match with any expectations set in the query logic.
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///   to use for finding the records.
    ///
    /// # Errors
    ///
    /// See [`Model::find_first()`].
    ///
    /// # See also
    ///
    /// * [`find()`](Model::find())
    /// * [`find_remote_ids()`](ModelExtension::find_remote_ids())
    ///
    async fn find_ids<Q>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> Result<Vec<Self::IdType>, StashError>
    where
        Q: Into<String> + Send,
    {
        tether
            .query_values::<_, Self::IdType>(
                formatdoc!(
                    "
                    SELECT
                        {} AS value
                    FROM
                        {}
                    {}
                    ",
                    Self::id_field_name(),
                    Self::table_name(),
                    query_logic.into(),
                ),
                params,
            )
            .await
    }

    /// Reloads the model from database.
    ///
    /// Especially useful for models which have `on_load` implementations
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    async fn reload(&mut self, tether: &Tether) -> Result<(), StashError> {
        if let Some(this) = Self::load(self.id_value()?, tether).await? {
            *self = this;
        }

        Ok(())
    }

    /// Deletes the model instance from database.
    ///
    /// # Errors
    ///
    /// When querying the database fails.
    ///
    async fn delete(self, bond: &Bond<'_>) -> Result<usize, StashError> {
        Self::delete_by_id(self.id_value()?, bond).await
    }

    /// Deletes the model instance from database.
    ///
    /// # Errors
    ///
    /// When querying the database fails.
    ///
    #[must_use]
    async fn delete_all(bond: &Bond<'_>) -> Result<usize, StashError> {
        let table = Self::table_name();
        let query = format!("DELETE FROM {table}");

        bond.execute(query, vec![]).await
    }
}

/// Extension trait for models where there is a relationship between a local id and remote id
/// for a resource.
///
/// This relationship usually exists when the given resource can be created locally without it
/// existing on the remote server. This relationship is expected to be expressed via
/// the [`crate::declare_local_id`] macro.
#[allow(async_fn_in_trait)]
pub trait ModelIdExtension: ModelExtension + Model<IdType: LocalIdMarker> {
    /// Remote Id type.
    type RemoteId: ProtonIdMarker;

    /// Return the remote id for this model.
    fn remote_id(&self) -> Option<&Self::RemoteId>;

    /// Returns whether this item has been synced.
    //
    /// An item is considered synced when it has a remote id that was assigned
    /// by the server.
    fn is_synced(&self) -> bool {
        self.remote_id().is_some()
    }

    /// Remote id field name.
    #[must_use]
    fn remote_id_field_name() -> &'static str {
        "remote_id"
    }

    /// Finds a record by its ID.
    ///
    /// The [`load()`](Model::load()) method is so-called to be the counterpart
    /// to [`save()`](Model::save()), but could equally be called `find_by_id()`
    /// under familiar naming conventions. The reason that was not used is that
    /// we have multiple ID types, and so "load" is a more generic term that is
    /// closely associated with local representations.
    ///
    /// However, there is a need to find records by their remote IDs, and indeed
    /// a need to *generically* find records regardless of ID type. Hence this
    /// method is provided to help with that. It is a convenience method that
    /// calls [`load()`](Id::load()) on the ID type itself.
    ///
    /// It does very little, and exists to formalise the interface for carrying
    /// out this process, for uniformity and centralisation of this common
    /// operation.
    ///
    /// # Parameters
    ///
    /// * `id`        - The ID of the record to find.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the record.
    ///
    /// # Errors
    ///
    /// See [`Model::find_first()`].
    ///
    /// # See also
    ///
    /// * [`find_first()`](Model::find_first())
    /// * [`load()`](Model::load())
    ///
    async fn find_by_remote_id(
        id: Self::RemoteId,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        Self::find_first(
            format!("WHERE {} =?", Self::remote_id_field_name()),
            params![id],
            tether,
        )
        .await
    }

    /// Finds records by its remote IDs.
    ///
    /// # Parameters
    ///
    /// * `ids`         - The IDs of the records to find
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the record.
    ///
    /// # Errors
    ///
    /// See [`Model::find_first()`].
    ///
    /// # See also
    ///
    /// * [`find_first()`](Model::find_first())
    /// * [`load()`](Model::load())
    /// * [`load_by_id()`](ModelExtension::load_by_id())
    ///
    async fn find_by_remote_ids(
        ids: impl IntoIterator<Item = Self::RemoteId>,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let mut ids = ids.into_iter().peekable();
        let field_name = if ids.peek().is_some() {
            Self::remote_id_field_name()
        } else {
            return Ok(vec![]);
        };
        #[allow(trivial_casts)]
        let parameters = ids
            .map(|i| Box::new(i) as Box<dyn ToSql + Send>)
            .collect_vec();
        let placeholders = stash::utils::placeholders(parameters.len());

        let query = format!("WHERE {field_name} IN ({placeholders})");
        Self::find(query, parameters, tether).await
    }

    /// Deletes a record by its remote ID.
    ///
    /// This method is a convenience method for deleting a record by its remote ID.
    ///
    /// # Returns
    ///
    /// Returns the number of rows deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to delete the account from the db.
    async fn delete_by_remote_id(
        remote_id: Self::RemoteId,
        bond: &Bond<'_>,
    ) -> Result<usize, StashError> {
        let table = Self::table_name();
        let query = format!(
            "DELETE FROM {table} WHERE {} = ?",
            Self::remote_id_field_name()
        );

        bond.execute(query, params![remote_id]).await
    }

    /// Return the local id counterpart for a given `remote_id`.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    async fn remote_id_counterpart(
        remote_id: Self::RemoteId,
        tether: &Tether,
    ) -> Result<Option<Self::IdType>, StashError> {
        match tether
            .query_value::<_, Self::IdType>(
                formatdoc!(
                    "
                            SELECT
                                {} AS value
                            FROM
                                {}
                            WHERE
                                {} = ?
                            LIMIT 1
                            ",
                    Self::id_field_name(),
                    Self::table_name(),
                    Self::remote_id_field_name(),
                ),
                params![remote_id],
            )
            .await
        {
            Ok(v) => Ok(Some(v)),
            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Return the local id counterparts for a given set of `remote ids`.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    #[must_use]
    async fn remote_ids_counterpart(
        remote_ids: Vec<Self::RemoteId>,
        tether: &Tether,
    ) -> Result<Vec<Self::IdType>, StashError> {
        let placeholders = stash::utils::placeholders(remote_ids.len());
        #[allow(trivial_casts)]
        let values = remote_ids
            .into_iter()
            .map(|id| Box::new(id) as Box<dyn ToSql + Send>)
            .collect();
        tether
            .query_values::<_, Self::IdType>(
                formatdoc!(
                    "
                            SELECT
                                {} AS value
                            FROM
                                {}
                            WHERE
                                {} IN ({})
                            ",
                    Self::id_field_name(),
                    Self::table_name(),
                    Self::remote_id_field_name(),
                    placeholders,
                ),
                values,
            )
            .await
    }

    /// Return the remote id counterpart for a given `local_id`.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    async fn local_id_counterpart(
        local_id: Self::IdType,
        tether: &Tether,
    ) -> Result<Option<Self::RemoteId>, StashError> {
        match tether
            .query_value::<_, Option<Self::RemoteId>>(
                formatdoc!(
                    "
                            SELECT
                                {} AS value
                            FROM
                                {}
                            WHERE
                                {} = ?
                            LIMIT 1
                            ",
                    Self::remote_id_field_name(),
                    Self::table_name(),
                    Self::id_field_name(),
                ),
                params![local_id],
            )
            .await
        {
            Ok(v) => Ok(v),
            Err(e) => {
                if matches!(
                    e,
                    StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                ) {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Return the remote id counterparts for a given set of `local ids`.
    ///
    /// # Error
    ///
    /// Returns error if the query failed.
    #[must_use]
    async fn local_ids_counterpart(
        local_ids: Vec<Self::IdType>,
        tether: &Tether,
    ) -> Result<Vec<Self::RemoteId>, StashError> {
        let placeholders = stash::utils::placeholders(local_ids.len());
        #[allow(trivial_casts)]
        let values = local_ids
            .into_iter()
            .map(|id| Box::new(id) as Box<dyn ToSql + Send>)
            .collect();
        tether
            .query_values::<_, Self::RemoteId>(
                formatdoc!(
                    "
                            SELECT
                                {} AS value
                            FROM
                                {}
                            WHERE
                                {} IN ({})
                            AND
                                {} IS NOT NULL
                            ",
                    Self::remote_id_field_name(),
                    Self::table_name(),
                    Self::id_field_name(),
                    placeholders,
                    Self::remote_id_field_name(),
                ),
                values,
            )
            .await
    }

    /// Finds remote record IDs matching given criteria.
    ///
    /// This method is the counterpart to [`find()`](Model::find()), but where
    /// only the remote IDs are needed. This saves having to load the entire
    /// model data in order to get the IDs. It operates in the same way as
    /// [`find()`](Model::find()). except it does not support live queries.
    ///
    /// # WARNING
    ///
    /// This method will **ONLY** work with models that have a `remote_id`
    /// field. If the model does not follow this convention, use a manual
    /// approach.
    ///
    /// # Parameters
    ///
    /// * `query_logic` - The query logic to use for finding the records. This
    ///   should be a string that represents the conditions,
    ///   ordering, offset, and limit for the query, as may be
    ///   required. It can be empty. Note that each part of the
    ///   logic is optional — so if conditions are passed, for
    ///   instance, the `WHERE` keyword needs to be included.
    /// * `params`      - The parameters to use in the query. These should be in
    ///   the order they are expected in the query logic, and
    ///   match with any expectations set in the query logic.
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///   to use for finding the records.
    ///
    /// # Errors
    ///
    /// See [`Model::find_first()`].
    ///
    /// # See also
    ///
    /// * [`find()`](Model::find())
    /// * [`find_local_ids()`](ModelExtension::find_local_ids())
    ///
    async fn find_remote_ids<Q>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> Result<Vec<Self::RemoteId>, StashError>
    where
        Q: Into<String> + Send,
    {
        tether
            .query_values::<_, Self::RemoteId>(
                formatdoc!(
                    "
                    SELECT
                        {} AS value
                    FROM
                        {}
                    {}
                    ",
                    Self::remote_id_field_name(),
                    Self::table_name(),
                    query_logic.into(),
                ),
                params,
            )
            .await
    }
}

impl<T: Model> ModelExtension for T {}

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

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("users")]
pub struct User {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[IdField(optional)]
    pub remote_id: Option<UserId>,

    /// TODO: Document this field.
    #[DbField]
    pub create_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub credit: i64,

    /// TODO: Document this field.
    #[DbField]
    pub currency: String,

    /// TODO: Document this field.
    #[DbField]
    pub delinquent: u32,

    /// TODO: Document this field.
    #[DbField]
    pub display_name: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub email: String,

    /// TODO: Document this field.
    #[DbField]
    pub keys: UserKeys,

    /// TODO: Document this field.
    #[DbField]
    pub flags: Flags,

    /// TODO: Document this field.
    #[DbField]
    pub max_space: i64,

    /// TODO: Document this field.
    #[DbField]
    pub max_upload: i64,

    /// TODO: Document this field.
    #[DbField]
    pub mnemonic_status: UserMnemonicStatus,

    /// TODO: Document this field.
    #[DbField]
    pub private: u32,

    /// TODO: Document this field.
    #[DbField]
    pub name: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub product_used_space: ProductUsedSpace,

    /// TODO: Document this field.
    #[DbField]
    pub role: u32,

    /// TODO: Document this field.
    #[DbField]
    pub services: u32,

    /// TODO: Document this field.
    #[DbField]
    pub subscribed: u32,

    /// TODO: Document this field.
    #[DbField]
    pub to_migrate: bool,

    /// TODO: Document this field.
    #[DbField]
    pub used_space: i64,

    /// TODO: Document this field.
    #[DbField]
    pub user_type: UserType,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl From<ApiUser> for User {
    fn from(value: ApiUser) -> Self {
        Self {
            remote_id: Some(value.id),
            create_time: value.create_time,
            credit: value.credit,
            currency: value.currency,
            delinquent: value.delinquent,
            display_name: value.display_name,
            email: value.email,
            keys: value.keys.into(),
            flags: value.flags.into(),
            max_space: value.max_space,
            max_upload: value.max_upload,
            mnemonic_status: value.mnemonic_status.into(),
            private: value.private,
            name: value.name,
            product_used_space: value.product_used_space.into(),
            role: value.role,
            services: value.services,
            subscribed: value.subscribed,
            to_migrate: value.to_migrate,
            used_space: value.used_space,
            user_type: value.user_type.into(),
            row_id: None,
        }
    }
}

impl User {
    // /// Get the user's display name.
    // #[must_use]
    // pub fn user_name(&self) -> &str {
    //     if let Some(display_name) = self.display_name.as_deref() {
    //         display_name
    //     } else if let Some(name) = self.name.as_deref() {
    //         name
    //     } else {
    //         &self.email
    //     }
    // }

    /// Save a user to the database.
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
            if let Some(existing) = Self::find_by_id(remote_id, bond).await? {
                self.row_id = existing.row_id;
            }
        }

        <Self as Model>::save(self, bond).await
    }

    /// Download and store user info and settings into the database
    ///
    /// # Parameters
    ///
    /// * `stash` - The database instance to store the addresses.
    /// * `api`   - The API instance to use to download the addresses.
    ///
    /// # Errors
    ///
    /// TODO: Document the errors.
    ///
    pub async fn sync_user_and_settings(api: &Proton, stash: &Stash) -> CoreContextResult<()> {
        let mut user = User::from(api.get_users().await?.user);
        let mut settings = UserSettings::from(api.get_settings().await?.user_settings);
        settings.remote_id.clone_from(&user.remote_id);

        let mut conn = stash.connection();
        let tx = conn.transaction().await?;
        user.save(&tx).await?;
        settings.save(&tx).await?;
        tx.commit().await?;

        Ok(())
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("user_settings")]
#[allow(clippy::struct_excessive_bools)]
pub struct UserSettings {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[IdField(optional)]
    pub remote_id: Option<UserId>,

    /// TODO: Document this field.
    #[DbField]
    pub crash_reports: bool,

    /// TODO: Document this field.
    #[DbField]
    pub date_format: DateFormat,

    /// TODO: Document this field.
    #[DbField]
    pub density: Density,

    /// TODO: Document this field.
    #[DbField]
    pub device_recovery: bool,

    /// TODO: Document this field.
    #[DbField]
    pub early_access: bool,

    /// TODO: Document this field.
    #[DbField]
    pub email: Email,

    /// TODO: Document this field.
    #[DbField]
    pub flags: SettingsFlags,

    /// TODO: Document this field.
    #[DbField]
    pub hide_side_panel: bool,

    /// TODO: Document this field.
    #[DbField]
    pub high_security: HighSecurity,

    /// TODO: Document this field.
    #[DbField]
    pub invoice_text: String,

    /// TODO: Document this field.
    #[DbField]
    pub locale: String,

    /// TODO: Document this field.
    #[DbField]
    pub log_auth: LogAuth,

    /// TODO: Document this field.
    #[DbField]
    pub news: u32,

    /// TODO: Document this field.
    #[DbField]
    pub password: Password,

    /// TODO: Document this field.
    #[DbField]
    pub phone: Phone,

    /// TODO: Document this field.
    #[DbField]
    pub referral: Option<Referral>,

    /// TODO: Document this field.
    #[DbField]
    pub session_account_recovery: bool,

    /// TODO: Document this field.
    #[DbField]
    pub telemetry: bool,

    /// TODO: Document this field.
    #[DbField]
    pub time_format: TimeFormat,

    /// TODO: Document this field.
    #[DbField]
    pub two_factor_auth: TwoFa,

    /// TODO: Document this field.
    #[DbField]
    pub week_start: WeekStart,

    /// TODO: Document this field.
    #[DbField]
    pub welcome: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl UserSettings {
    /// Save a user's settings to the database.
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
            if let Some(existing) = Self::find_by_id(remote_id, bond).await? {
                self.row_id = existing.row_id;
            }
        }

        <Self as Model>::save(self, bond).await
    }
}

impl From<ApiUserSettings> for UserSettings {
    fn from(value: ApiUserSettings) -> Self {
        Self {
            remote_id: None,
            crash_reports: value.crash_reports,
            date_format: value.date_format.into(),
            density: value.density.into(),
            device_recovery: value.device_recovery,
            early_access: value.early_access,
            email: value.email.into(),
            flags: value.flags.into(),
            hide_side_panel: value.hide_side_panel,
            high_security: value.high_security.into(),
            invoice_text: value.invoice_text,
            locale: value.locale,
            log_auth: value.log_auth.into(),
            news: value.news,
            password: value.password.into(),
            phone: value.phone.into(),
            referral: value.referral.map(Into::into),
            session_account_recovery: value.session_account_recovery,
            telemetry: value.telemetry,
            time_format: value.time_format.into(),
            two_factor_auth: value.two_factor_auth.into(),
            week_start: value.week_start.into(),
            welcome: value.welcome,
            row_id: None,
        }
    }
}
