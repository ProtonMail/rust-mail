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
pub mod sender_image_cache;

pub use self::contact::*;
pub use self::contact_card::*;
pub use self::contact_email::*;

use crate::datatypes::{
    AddressKeys, AddressSignedKeyList, AddressStatus, AddressType, AgnosticId, DateFormat, Density,
    Email, Flags, HighSecurity, Id, LocalId, LogAuth, Password, Phone, ProductUsedSpace,
    QueryResultRemoteId, Referral, RemoteId, SettingsFlags, TimeFormat, TwoFa, UserKeys,
    UserMnemonicStatus, UserType, WeekStart,
};
use crate::CoreContextResult;
use flume::Sender as QueueSender;
use indoc::formatdoc;
use proton_api_core::services::proton::response_data::{
    Address as ApiAddress, User as ApiUser, UserSettings as ApiUserSettings,
};
use proton_api_core::services::proton::Proton;
use stash::exports::ToSql;
use stash::macros::Model;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError};

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
    ///                   to use for finding the records.
    /// * `queue`       - An optional queue to send changes to. If this is
    ///                   provided, the function will listen for changes to the
    ///                   result set and send them to the queue. This is useful
    ///                   for live updates.
    ///
    /// # Errors
    ///
    /// See [`Model::find()`].
    ///
    /// # See also
    ///
    /// * [`find()`](Model::find())
    ///
    async fn all<A>(
        interface: &A,
        queue: Option<QueueSender<ResultsetChange<Self, Self::IdType>>>,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Self::find(String::new(), vec![], &interface.clone().into(), queue).await
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
    ///                 use for finding the record.
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
    async fn find_by_id<I, A>(id: I, interface: &A) -> Result<Option<Self>, StashError>
    where
        I: Into<AgnosticId> + Id,
        A: Into<AgnosticInterface> + Interface,
    {
        id.load(interface).await
    }

    /// Finds a records by its IDs.
    /// Only `#[IdField]` is supported as it uses `find` method.
    ///
    /// # Parameters
    ///
    /// * `ids`         - The IDs of the records to find
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the record.
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
    async fn find_by_ids<I, A>(ids: Vec<I>, interface: &A) -> Result<Vec<Self>, StashError>
    where
        I: Into<AgnosticId> + Id + ToSql + 'static,
        A: Into<AgnosticInterface> + Interface,
    {
        let query = format!(
            "WHERE {} IN ({})",
            Self::id_field_name(),
            vec!["?"; ids.len()].join(","),
        );
        let params: Vec<Box<_>> = ids
            .into_iter()
            .map(|id| {
                let boxed_id: Box<dyn ToSql + Send> = Box::new(id);
                boxed_id
            })
            .collect();

        Self::find(query, params, interface, None).await
    }

    /// Finds local record IDs matching given criteria.
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
    ///                   should be a string that represents the conditions,
    ///                   ordering, offset, and limit for the query, as may be
    ///                   required. It can be empty. Note that each part of the
    ///                   logic is optional — so if conditions are passed, for
    ///                   instance, the `WHERE` keyword needs to be included.
    /// * `params`      - The parameters to use in the query. These should be in
    ///                   the order they are expected in the query logic, and
    ///                   match with any expectations set in the query logic.
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///                   to use for finding the records.
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
    async fn find_local_ids<Q, A>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        interface: &A,
    ) -> Result<Vec<LocalId>, StashError>
    where
        Q: Into<String> + Send,
        A: Into<AgnosticInterface> + Interface,
    {
        Ok(interface
            .query_values::<_, u64>(
                formatdoc!(
                    "
                    SELECT
                        local_id AS value
                    FROM
                        {}
                    {}
                    ",
                    Self::table_name(),
                    query_logic.into(),
                ),
                params,
            )
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
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
    ///                   should be a string that represents the conditions,
    ///                   ordering, offset, and limit for the query, as may be
    ///                   required. It can be empty. Note that each part of the
    ///                   logic is optional — so if conditions are passed, for
    ///                   instance, the `WHERE` keyword needs to be included.
    /// * `params`      - The parameters to use in the query. These should be in
    ///                   the order they are expected in the query logic, and
    ///                   match with any expectations set in the query logic.
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///                   to use for finding the records.
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
    async fn find_remote_ids<Q, A>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        interface: &A,
    ) -> Result<Vec<RemoteId>, StashError>
    where
        Q: Into<String> + Send,
        A: Into<AgnosticInterface> + Interface,
    {
        Ok(interface
            .query::<_, QueryResultRemoteId>(
                formatdoc!(
                    "
                    SELECT
                        remote_id AS id
                    FROM
                        {}
                    {}
                    ",
                    Self::table_name(),
                    query_logic.into(),
                ),
                params,
            )
            .await?
            .into_iter()
            .map(|r| r.id)
            .collect())
    }

    /// Deletes a record by its remote ID.
    ///
    /// This method is a convenience method for deleting a record by its remote ID.
    /// It assumes the model has a `remote_id` field; if it does not, the stash
    /// will return an error.
    ///
    /// # Returns
    ///
    /// Returns the number of rows deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to delete the account from the db.
    async fn delete_by_remote_id<A>(remote_id: RemoteId, interface: &A) -> Result<usize, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let table = Self::table_name();
        let query = format!("DELETE FROM {table} WHERE remote_id = ?");

        interface.execute(query, params![remote_id]).await
    }

    /// Counts models in database.
    ///
    /// # Parameters
    ///
    /// * `query_logic` - The query logic to use for finding the records. This
    ///                   should be a string that represents the conditions,
    ///                   ordering, offset, and limit for the query, as may be
    ///                   required. It can be empty. Note that each part of the
    ///                   logic is optional — so if conditions are passed, for
    ///                   instance, the `WHERE` keyword needs to be included.
    /// * `params`      - The parameters to use in the query. These should be in
    ///                   the order they are expected in the query logic, and
    ///                   match with any expectations set in the query logic.
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///                   to use for finding the records.
    ///
    /// # Errors
    ///
    /// When querying the database fails.
    ///
    async fn count<Q, A>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        interface: &A,
    ) -> Result<u64, StashError>
    where
        Q: Into<String> + Send,
        A: Into<AgnosticInterface> + Interface,
    {
        interface
            .query_value::<_, u64>(
                formatdoc!(
                    "SELECT COUNT(*) AS value FROM {} {}",
                    Self::table_name(),
                    query_logic.into(),
                ),
                params,
            )
            .await
    }

    /// Sets the stash, returning the updated model.
    #[must_use]
    fn with_stash(mut self, stash: &Stash) -> Self {
        self.set_stash(stash);
        self
    }

    /// Saves the model by value, returning the updated model.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    async fn with_save(mut self) -> Result<Self, StashError> {
        self.save().await?;
        Ok(self)
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
    pub local_id: Option<LocalId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<RemoteId>,

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

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl Address {
    /// Save an address to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save an address to the database.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, interface).await? {
                self.row_id = existing.row_id;
                self.local_id = existing.local_id;
            }
        }

        <Self as Model>::save_using(self, interface).await
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

        let tx = stash.transaction().await?;
        for mut address in addresses {
            address.set_stash(stash);
            address.save_using(&tx).await?;
        }
        tx.commit().await?;

        Ok(())
    }
}

impl From<ApiAddress> for Address {
    fn from(value: ApiAddress) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
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
            stash: None,
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
    pub remote_id: Option<RemoteId>,

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

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl From<ApiUser> for User {
    fn from(value: ApiUser) -> Self {
        Self {
            remote_id: Some(value.id.into()),
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
            stash: None,
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
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save a user to the database.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, interface).await? {
                self.row_id = existing.row_id;
            }
        }

        <Self as Model>::save_using(self, interface).await
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
        user.set_stash(stash);
        settings.set_stash(stash);
        let tx = stash.transaction().await?;
        user.save_using(&tx).await?;
        settings.save_using(&tx).await?;
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
    pub remote_id: Option<RemoteId>,

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

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl UserSettings {
    /// Save a user's settings to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save a user's settings to the database.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, interface).await? {
                self.row_id = existing.row_id;
            }
        }

        <Self as Model>::save_using(self, interface).await
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
            stash: None,
        }
    }
}
