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

mod address;
mod app_settings;
pub mod contact;
mod contact_card;
mod contact_email;
mod initialized_components;
mod labels;
mod user;
mod user_settings;

pub use self::address::*;
pub use self::app_settings::*;
pub use self::contact::*;
pub use self::contact_card::*;
pub use self::contact_email::*;
pub use self::initialized_components::*;
pub use self::labels::*;
pub use self::user::*;
pub use self::user_settings::*;

use crate::datatypes::LocalIdMarker;
use indoc::formatdoc;
use itertools::Itertools as _;
use proton_core_api::services::proton::ProtonIdMarker;
use stash::exports::{FromSql, SqliteError, ToSql};
use stash::orm::Model;
use stash::params;
use stash::rusqlite::params_from_iter;
use stash::rusqlite::{Connection, OptionalExtension};
use stash::stash::{Bond, StashError, Tether};
use stash::utils::{ConnectionExt, IterMapToSql as _, MapToSql as _, placeholders};

#[allow(async_fn_in_trait)]
pub trait ModelExtension: Model {
    /// Finds all records in the database.
    ///
    /// This is a convenience method for when all records need to be loaded
    /// without any criteria. This happens remarkably often, and centralises the
    /// functionality, plus makes the intent clear.
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
            Self::id_field_name()
        } else {
            return Ok(vec![]);
        };
        #[allow(trivial_casts)]
        let parameters = ids
            .map(|i| Box::new(i) as Box<dyn ToSql + Send>)
            .collect_vec();
        let placeholders = placeholders(&parameters);

        let query = format!("WHERE {field_name} IN ({placeholders})");
        Self::find(query, parameters, tether).await
    }

    /// Saves the model by value, returning the updated model.
    async fn with_save(mut self, bond: &Bond<'_>) -> Result<Self, StashError> {
        self.save(bond).await?;
        Ok(self)
    }

    /// Forcefully inserts the model by value, returning the updated model.
    async fn with_insert(mut self, bond: &Bond<'_>) -> Result<Self, StashError> {
        self.insert(bond).await?;
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

    /// Deletes records by its IDs.
    ///
    /// This method is a convenience method for deleting multiple records by its primary id.
    ///
    /// # Returns
    ///
    /// Returns the number of rows deleted.
    ///
    /// # Errors
    ///
    /// Returns an error if we fail to delete the item from the database.
    #[allow(trivial_casts)]
    #[must_use]
    async fn delete_by_ids(ids: Vec<Self::IdType>, bond: &Bond<'_>) -> Result<usize, StashError> {
        let table = Self::table_name();
        let params = ids
            .into_iter()
            .map(|param| Box::new(param) as Box<dyn ToSql + Send>)
            .collect::<Vec<_>>();
        let placeholders = placeholders(&params);
        let query = format!(
            "DELETE FROM {table} WHERE {} IN ({placeholders})",
            Self::id_field_name(),
        );

        bond.execute(query, params).await
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
                        {}
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

    /// Checks if the model exists in the database.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    async fn exists(&self, tether: &Tether) -> Result<bool, StashError> {
        tether
            .query_values::<_, Self::IdType>(
                formatdoc!(
                    "SELECT {} FROM {} WHERE {} = ? LIMIT 1",
                    Self::id_field_name(),
                    Self::table_name(),
                    Self::id_field_name(),
                ),
                params![self.id_value()?],
            )
            .await
            .map(|v| !v.is_empty())
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
    type RemoteId: ProtonIdMarker + ToSql + FromSql;

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
        tether
            .sync_query(move |conn| Self::find_by_remote_id_sync(&id, conn))
            .await
    }

    fn find_by_remote_id_sync(
        id: &Self::RemoteId,
        conn: &Connection,
    ) -> Result<Option<Self>, StashError> {
        Self::find_first_sync(
            format!("WHERE {} =?", Self::remote_id_field_name()),
            (id,),
            conn,
        )
    }

    fn find_by_remote_ids_sync(
        ids: &[Self::RemoteId],
        conn: &Connection,
    ) -> Result<Vec<Self>, StashError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let field_name = Self::remote_id_field_name();
        let placeholders = placeholders(ids);

        Self::find_sync(
            format!("WHERE {field_name} IN ({placeholders})"),
            params_from_iter(ids),
            conn,
        )
    }

    /// Finds records by its remote IDs.
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
        let ids = ids.bridge_sql();
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let field_name = Self::remote_id_field_name();
        let placeholders = placeholders(&ids);

        Self::find(
            format!("WHERE {field_name} IN ({placeholders})"),
            ids,
            tether,
        )
        .await
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

    /// Return the local id for a given `remote_id`.
    async fn remote_id_counterpart(
        remote_id: Self::RemoteId,
        tether: &Tether,
    ) -> Result<Option<Self::IdType>, StashError> {
        tether
            .sync_query(move |c| Self::remote_id_counterpart_sync(&remote_id, c))
            .await
    }

    /// Return the local id for a given `remote_id`.
    fn remote_id_counterpart_sync(
        remote_id: &Self::RemoteId,
        conn: &Connection,
    ) -> Result<Option<Self::IdType>, StashError> {
        conn.query_row_col(
            formatdoc! {
                "
                SELECT {} FROM {} WHERE {} = ?
                LIMIT 1
                ",
                Self::id_field_name(),
                Self::table_name(),
                Self::remote_id_field_name(),
            },
            (remote_id,),
        )
        .optional()
        .map_err(StashError::from)
    }

    /// Return the local id for a given `remote_id`.
    fn remote_ids_counterpart_sync(
        remote_ids: &[Self::RemoteId],
        conn: &Connection,
    ) -> Result<Vec<Self::IdType>, StashError> {
        conn.query_rows_col(
            formatdoc! {
                "
                SELECT {} FROM {} WHERE {} IN ({})
                ",
                Self::id_field_name(),
                Self::table_name(),
                Self::remote_id_field_name(),
                placeholders(remote_ids),
            },
            params_from_iter(remote_ids),
        )
        .map_err(StashError::from)
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
        tether
            .query_values(
                formatdoc!(
                    "
                            SELECT
                                {}
                            FROM
                                {}
                            WHERE
                                {} IN ({})
                            ",
                    Self::id_field_name(),
                    Self::table_name(),
                    Self::remote_id_field_name(),
                    placeholders(&remote_ids),
                ),
                remote_ids.to_sql(),
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
                                {}
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
        tether
            .query_values(
                formatdoc!(
                    "
                            SELECT
                                {}
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
                    placeholders(&local_ids),
                    Self::remote_id_field_name(),
                ),
                local_ids.to_sql(),
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
                        {}
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
