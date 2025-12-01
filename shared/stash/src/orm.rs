#![allow(async_fn_in_trait)]

//! ORM utilities for working with database records.
//!
//! This module provides a set of traits and structs for working with database
//! records. It is used to define the interface for database records, and to
//! provide methods for loading and saving records from the database.
//!
//! Nothing prevents the database-handling interface from being used directly,
//! but it is lower-level. The ORM layer sits on top of the database-handling
//! layer and provides a more convenient and idiomatic interface for working
//! with types that are saved to the database.
//!

use crate::params;
use crate::stash::{Bond, StashError, StashResult, Tether};
use crate::utils::ConnectionExt;
use core::any::Any;
use core::fmt::{Debug, Display};
use core::future::Future;
use indoc::formatdoc;
use rusqlite::types::FromSql;
use rusqlite::{Connection, Error as SqliteError, Row, ToSql, Transaction};
use rusqlite::{Params, params_from_iter};
use serde::de::Error as DeserializationError;
use serde::ser::Error as SerializationError;
use std::vec::IntoIter;
use thiserror::Error;
use tracing::error;

/// Errors for conversion of database row data into record types.
#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum ConversionError {
    /// For some reason it is not possible to obtain a name for a particular
    /// column. This refers specifically to trying to obtain the information
    /// from the database query results, and technically should never happen, as
    /// it would mean there is a column present in the resultset without a name.
    #[error("Column {0}'s name is not available: {1}")]
    ColumnNameNotAvailable(usize, SqliteError),

    /// For some reason it is not possible to obtain column names. This refers
    /// specifically to trying to obtain the information from the database query
    /// results.
    #[error("Column names are not available")]
    ColumnNamesNotAvailable,

    /// Basic deserialisation error from [`serde`].
    #[error("Deserialization error{}: {1}", .0.as_ref().map(|column| format!(r#" for column "{column}""#)).unwrap_or_default())]
    DeserializationError(Option<String>, String),

    /// SQL type conversion error from [`rusqlite`], specifically when trying to
    /// convert a value from the database into a Rust type using the [`FromSql`]
    /// implementation.
    #[error("FromSql conversion error for column \"{0}\": {1}")]
    FromSqlConversionError(String, SqliteError),

    /// The row data returned from the database query is missing a column
    /// according to the expectations of the record type.
    #[error("Missing column: \"{0}\"")]
    MissingColumn(String),

    /// SQL-related error from [`rusqlite`].
    #[error("SQLite error: {0}")]
    SqliteError(#[from] SqliteError),

    /// Basic serialisation error from [`serde`].
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl DeserializationError for ConversionError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::DeserializationError(None, msg.to_string())
    }
}

impl SerializationError for ConversionError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::SerializationError(msg.to_string())
    }
}

/// Notification of changes to a resultset.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ResultsetChange<T: Model, I: ToSql> {
    /// A record has been deleted from the resultset.
    Deleted(I),

    /// A new record has been added to the resultset.
    Inserted(T),

    /// A record has been updated in the resultset.
    Updated(T),
}

/// A trait for simple database records.
///
/// This trait is used to define the interface for simple database records,
/// based around converting results from a query into a specific type `T`.
///
/// For more involved functionality, see [`Model`].
///
/// # Design
///
/// The intention is that the various data fields for the struct should be
/// mapped to the database fields using serde, which will be used for
/// serialisation and deserialisation. A common pattern is for those fields to
/// be public on the struct, but the actual method of management is up to the
/// implementor. Meanwhile, the associated [`Stash`] would usually be stored in
/// a private `stash` field — but this again is up to the implementor.
///
/// # See also
///
/// * [`Model`]
///
pub trait DbRecord: Clone + Debug + PartialEq + Send + Sized + Sync
where
    Self: 'static,
{
    /// Gets a list of field values for the record.
    ///
    /// The field values are returned in a form that is compatible with
    /// conversion to SQL type, but pre-conversion.
    ///
    /// Note: Any fields using an intermediary type (i.e. specified with the
    /// `via` attribute argument) will be converted to that type before being
    /// returned.
    ///
    fn field_values(&self) -> impl Iterator<Item = &dyn ToSql> + '_;

    /// Converts a row from the database into a record.
    ///
    /// This function is used to convert a row from the database from primitive
    /// SQL types into a Rust record type. It is used to convert the results of
    /// a query into a specific type `T`.
    ///
    fn from_row(row: &Row<'_>) -> Result<Self, ConversionError>;

    fn model_find(
        query: impl AsRef<str>,
        params: impl Params,
        conn: &Connection,
    ) -> StashResult<Vec<Self>> {
        let mut stmt = conn.prepare_cached(query.as_ref())?;
        let records = stmt
            .query_and_then(params, Self::from_row)?
            .collect::<Result<_, _>>()?;
        Ok(records)
    }

    fn model_find_first(
        query: impl AsRef<str>,
        params: impl Params,
        conn: &Connection,
    ) -> StashResult<Option<Self>> {
        let mut stmt = conn.prepare_cached(query.as_ref())?;
        let records = stmt
            .query_and_then(params, Self::from_row)?
            .next()
            .transpose()?;
        Ok(records)
    }
}

/// A trait for fully-modelled database records.
///
/// This trait is used to define the interface for fully-modelled database
/// records. It provides methods for loading and saving records from the
/// database.
///
/// For simpler functionality, see [`DbRecord`].
///
/// # Design
///
/// The intention is that the various data fields for the struct should be
/// mapped to the database fields using serde, which will be used for
/// serialisation and deserialisation. A common pattern is for those fields to
/// be public on the struct, but the actual method of management is up to the
/// implementor. Meanwhile, the associated [`Stash`] would usually be stored in
/// a private `stash` field — but this again is up to the implementor.
///
/// # See also
///
/// * [`DbRecord`]
///
pub trait Model: DbRecord + ModelHooks
where
    Self: 'static,
    <Self as Model>::Id: Send + Sync + 'static,
{
    /// The ID type for the record. This is the type as stored in the struct,
    /// i.e. the field type. For an optional ID, this includes the [`Option`].
    type Id: Clone + Debug + FromSql + PartialEq + ToSql;

    /// The actual ID type for the record. This is the type as stored in the
    /// database. For an optional ID, this does *not* include the [`Option`] —
    /// for non-optional IDs, it is the same as [`Self::Id`].
    type IdType: Clone + Debug + FromSql + PartialEq + ToSql + Send + Sync;

    const INSERT_QUERY: &str;
    const UPDATE_QUERY: &str;
    const COUNT_QUERY: &str;
    const DELETE_BY_ID_QUERY: &str;

    /// Finds records in the database using specific query logic.
    ///
    /// This function bridges the gap between ORM-level handling of formalised
    /// records and the extended functionality that is available when
    /// interacting directly with the underlying database service layer. The
    /// primary ambition is to build on what is possible with the [`query()`](Stash::query())
    /// function and combine common actions to reduce boilerplate code.
    ///
    /// Notably, the most important aspect of its use is that it accepts "query
    /// logic" in order to find results. Query logic is NOT a full query, and
    /// has the following expectations:
    ///
    ///   1. The fields returned will be managed by the "find" subsystem, and
    ///      will only ever be in context to one database table. Joins are not
    ///      supported — for anything involving joins or more complex queries,
    ///      the [`query()`](Stash::query()) function should be used directly.
    ///
    ///   2. The query logic is therefore everything from the `WHERE` clause in
    ///      the resulting SQL query, which can include conditions, ordering,
    ///      offset, and limit. It is essentially a full query but with the
    ///      restrictions noted in point 1.
    ///
    ///   3. All parameters given in the query logic should have a corresponding
    ///      value in the `params` argument. This is not managed in any
    ///      particularly-sophisticated way, and is simply a list of values that
    ///      will be passed to the query in the order they are given.
    ///
    /// This approach makes it possible for the "find" functionality to provide
    /// the ability to extract whichever fields it needs, which is important
    /// when subscribing to live resultset changes.
    ///
    /// Note that the [`params!`](crate::utils::params) macro is available to
    /// help shorten the syntax for passing in the query parameters.
    ///
    /// # Live change feed
    ///
    /// When listening for changes, the "find" functionality will handle them
    /// efficiently. Adding or changing data will trigger the re-running of the
    /// original query, BUT only the IDs will be returned, instead of all record
    /// data. This is therefore efficient and performant. Those IDs will then be
    /// compared with the original resultset, and any changes will be sent to
    /// the caller via the supplied queue.
    ///
    /// It is not possible to avoid re-running the query for an `INSERT` or
    /// `UPDATE`, but a `DELETE` will not re-run the query. Instead, if it is on
    /// the list of original IDs, a notification will be sent. Note that it is
    /// technically possible to restrict the scope of re-running the query to
    /// check only the record ID indicated in the database change event, but
    /// this has been left for a future optimisation step in order to avoid
    /// adding manipulation of the provided query logic for now.
    ///
    /// # Caveats
    ///
    /// This function is somewhat of a compromise in a number of ways:
    ///
    ///   1. It would be nice to be able to have a single "find" method that
    ///      allows finding many results, the first result, or a single result
    ///      by ID. However, this would lead to a difference in the return type,
    ///      which would be problematic. Instead, "find by ID" is handled via
    ///      the [`load()`](Model::load()) method, "find the first result" is
    ///      handled via the [`find_first()`](Model::find_first()) method, and
    ///      "find many results" is handled via this method.
    ///
    ///   2. The manner of specifying options for the search is crude. A more
    ///      sophisticated ORM implementation would allow for formal
    ///      representation of conditions, ordering, offset, limit, and so on.
    ///      But then we might as well just go and use one of those ORMs. As
    ///      this ORM is minimal by design, and targeted at specific use cases,
    ///      the functionality implemented here is carefully crafted to satisfy
    ///      the requirements of those use cases while keeping things easy to
    ///      use and performant.
    ///
    /// Notably, the "find" functionality has only been implemented for the
    /// [`Model`] trait, and not the [`DbRecord`] trait. This is because much of
    /// the benefit of using it comes from IDs, and the [`DbRecord`] trait does
    /// not require an ID field.
    ///
    /// # Parameters
    ///
    /// * `query_logic` - The query logic to use for finding the records. This
    ///   should be a string that represents the conditions,
    ///   ordering, offset, and limit for the query, as may be
    ///   required. It can be empty. Note that each part of the
    ///   logic is optional — so if conditions are passed, for
    ///   instance, the `WHERE` keyword needs to be included.
    fn find(
        query_logic: impl AsRef<str>,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> impl Future<Output = Result<Vec<Self>, StashError>> + Send {
        let query = format!(
            "SELECT * FROM {table} {query_logic}",
            query_logic = query_logic.as_ref(),
            table = Self::table_name(),
        );
        Self::load_inner(query, params, tether)
    }

    /// Finds the first record in a result set using specific query logic.
    ///
    /// This function is syntactic sugar for calling [`find()`](Model::find())
    /// with a limit set to 1, and then taking the first result. It is useful
    /// when only one result is expected.
    ///
    /// It behaves in the same way as [`find()`](Model::find()) otherwise
    /// (except that it does not support listening for changes). For more
    /// information, see the documentation for that function.
    ///
    /// # WARNING
    ///
    /// Note that this function adds a `LIMIT 1` to the query logic, so do not
    /// use this function if you have anything in your query that would conflict
    /// with this.
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
    fn find_first(
        query_logic: impl AsRef<str>,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> impl Future<Output = Result<Option<Self>, StashError>> + Send {
        let query = format!("{query_logic} LIMIT 1", query_logic = query_logic.as_ref());

        async move { Ok(Self::find(&query, params, tether).await?.into_iter().next()) }
    }

    async fn find_local_id_by(
        tether: &Tether,
        query_logic: impl AsRef<str>,
        params: Vec<Box<dyn ToSql + Send>>,
    ) -> Result<Vec<Self::IdType>, StashError> {
        let query = format!(
            "SELECT {local_id} FROM {table_name} {query_logic}",
            table_name = Self::table_name(),
            local_id = Self::id_field_name(),
            query_logic = query_logic.as_ref(),
        );

        tether.query_values(query, params).await
    }

    /// Gets the record's local id.
    ///
    /// # Panics
    ///
    /// This function will panic if the local id has not been set (i.e. if the
    /// model hasn't been saved yet).
    fn id(&self) -> Self::IdType;

    /// Gets the name of the ID field for the record type.
    ///
    /// This is the primary key column name for the record as defined when
    /// creating the table.
    fn id_field_name() -> &'static str;

    #[must_use]
    async fn load(id: Self::IdType, tether: &Tether) -> Result<Option<Self>, StashError> {
        let query = formatdoc! {"
            SELECT * FROM {table}
            WHERE {id} = ?
            LIMIT 1
            ",
            table = Self::table_name(),
            id = Self::id_field_name(),
        };

        Ok(Self::load_inner(query, params![id], tether)
            .await?
            .into_iter()
            .next())
    }

    /// Loads the model and calls after_load hook
    /// TODO: rename to load
    #[must_use]
    fn load_inner(
        query: impl Into<String>,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> impl Future<Output = Result<Vec<Self>, StashError>> + Send {
        let query = query.into();
        tether.sync_query(move |tx| Self::load_sync(query, params_from_iter(params), tx))
    }

    fn find_sync(
        query: impl AsRef<str>,
        params: impl Params,
        conn: &Connection,
    ) -> StashResult<Vec<Self>> {
        let query = format!(
            "SELECT * FROM {table} {query_logic}",
            query_logic = query.as_ref(),
            table = Self::table_name(),
        );
        Self::load_sync(query, params, conn)
    }

    fn load_sync(
        query: impl AsRef<str>,
        params: impl Params,
        conn: &Connection,
    ) -> StashResult<Vec<Self>> {
        let mut records = Self::model_find(query, params, conn)?;
        for i in &mut records {
            i.after_load(conn)?;
        }

        Ok(records)
    }

    fn find_first_sync(
        query: impl AsRef<str>,
        params: impl Params,
        conn: &Connection,
    ) -> StashResult<Option<Self>> {
        let query = format!(
            "SELECT * FROM {table} {query_logic} LIMIT 1",
            query_logic = query.as_ref(),
            table = Self::table_name(),
        );
        let mut record = Self::model_find_first(query, params, conn)?;
        if let Some(record) = &mut record {
            record.after_load(conn)?;
        }
        Ok(record)
    }

    fn load_by_id_sync(id: Self::IdType, conn: &Connection) -> StashResult<Option<Self>> {
        let query = format!("WHERE {id} = ?", id = Self::id_field_name());

        Self::find_first_sync(query, (id,), conn)
    }

    fn load_by_id_exact_sync(id: Self::IdType, conn: &Connection) -> StashResult<Self> {
        Self::load_by_id_sync(id, conn)
            .transpose()
            .ok_or_else(|| StashError::QueryReturnedNoRows)?
    }

    /// Saves a record to the database.
    /// If it has a local id it will update the record, otherwise it will insert it.
    ///
    fn insert_sync(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        let id: Self::IdType =
            tx.query_row_col(Self::INSERT_QUERY, params_from_iter(self.field_values()))?;

        self.set_id_value(id);

        self.after_save(tx)?;
        Ok(())
    }

    async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        let mut this = self.clone();
        *self = bond
            .sync_bridge(move |tx| {
                this.save_sync(tx)?;
                Ok(this)
            })
            .await?;
        Ok(())
    }

    async fn update(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        let mut this = self.clone();
        *self = bond
            .sync_bridge(move |tx| {
                this.update_sync(tx)?;
                Ok(this)
            })
            .await?;
        Ok(())
    }

    /// Forcefully insert, even if it has the ID set.
    async fn insert(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        let mut this = self.clone();
        *self = bond
            .sync_bridge(move |tx| {
                this.insert_sync(tx)?;
                Ok(this)
            })
            .await?;
        Ok(())
    }

    fn update_sync(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        let mut query = tx.prepare_cached(Self::UPDATE_QUERY)?;
        let id = self.id();
        let params = self.field_values().chain([&id as &dyn ToSql]);

        let affected: usize = query.execute(params_from_iter(params))?;

        if affected == 0 {
            return Err(StashError::NoRowsUpdated);
        }
        self.after_save(tx)?;
        Ok(())
    }

    fn save_sync(&mut self, tx: &Transaction<'_>) -> StashResult<()> {
        self.before_save(tx)?;
        //
        // HACK: This is not great but we're forced to do it since there's no guarantee that the
        // row does or doesn't exist.
        if let Ok(id) = self.id_value()
            && tx.query_row_col::<u64>(Self::COUNT_QUERY, (id,))? != 0
        {
            return self.update_sync(tx);
        }
        self.insert_sync(tx)
    }

    /// Gets the name of the table for the record type.
    fn table_name() -> &'static str;

    fn all_count(tether: &Tether) -> impl Future<Output = Result<u64, StashError>> + Send {
        async move { Self::count("", vec![], tether).await }
    }

    /// Counts models in database.
    fn count<Q>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> impl Future<Output = Result<u64, StashError>> + Send
    where
        Q: Into<String>,
    {
        let query_logic = query_logic.into();
        tether.sync_query(move |tx| Self::count_sync(&query_logic, params_from_iter(params), tx))
    }

    /// Counts models in database.
    fn count_sync(query_logic: &str, params: impl Params, conn: &Connection) -> StashResult<u64> {
        conn.query_row_col::<u64>(
            formatdoc!(
                "SELECT COUNT(*) FROM {} {}",
                Self::table_name(),
                query_logic,
            ),
            params,
        )
        .map_err(Into::into)
    }

    /// Gets the next id for the record type for manual id management.
    ///
    fn next_id(tether: &Tether) -> impl Future<Output = Result<Self::IdType, StashError>> + Send {
        async move {
            let query = formatdoc! {"
                SELECT COALESCE(MAX({id}), 0) + 1
                FROM {table}
                ",
                table = Self::table_name(),
                id = Self::id_field_name(),
            };
            tether.query_value::<_, Self::IdType>(query, vec![]).await
        }
    }

    fn id_value(&self) -> Result<Self::IdType, StashError>;
    fn set_id_value(&mut self, id: Self::IdType);
}

/// A collection of database records.
///
/// This struct is used to represent a collection of [`DbRecord`]s returned from
/// a query — the converted query results, i.e. the [`Rows`] that have been
/// converted into the desired type `T` — but boxed as [`Any`] so that they can
/// be returned via the oneshot channel. These are downcast immediately at the
/// other end of the channel.
///
/// Note that these can be [`DbRecord`]s or [`Model`]s, as the [`DbRecord`]
/// trait is a supertrait of [`Model`].
///
/// For more information on how this works, see the documentation for
/// `stash::converter()` (note: this is not a public function).
///
#[derive(Debug)]
pub struct DbRecords(pub(crate) Vec<Box<dyn Any + Send + 'static>>);

impl<T: 'static + Send> FromIterator<Box<T>> for DbRecords {
    fn from_iter<I: IntoIterator<Item = Box<T>>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|item| {
                    #[allow(clippy::shadow_same)]
                    let item: Box<dyn Any + Send> = item;
                    item
                })
                .collect(),
        )
    }
}

impl IntoIterator for DbRecords {
    type Item = Box<dyn Any + Send + 'static>;
    type IntoIter = IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// This provides hooks that will be called before or after [`Model::load`] and [`Model::save`].
/// These won't get called with fns like [`Tether::query`] and friends.
/// To use these, you just need to derive model with the `ModelHooks` attribute and impl the trait
/// manually.
pub trait ModelHooks {
    fn after_load(&mut self, _: &Connection) -> StashResult<()> {
        Ok(())
    }

    fn before_save(&mut self, _: &Transaction<'_>) -> StashResult<()> {
        Ok(())
    }

    fn after_save(&mut self, _: &Transaction<'_>) -> StashResult<()> {
        Ok(())
    }
}
