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

use crate::datatypes::QueryResultIdPair;
use crate::stash::{Bond, StashError, Tether};
use core::any::Any;
use core::fmt::{Debug, Display};
use core::future::Future;
use core::iter::once;
use indoc::formatdoc;
use itertools::Itertools as _;
use rusqlite::types::FromSql;
use rusqlite::{Error as SqliteError, Row, Rows, ToSql};
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
    /// Gets a list of field names for the record type.
    fn field_names() -> Vec<&'static str>;

    /// Gets a list of field values for the record.
    ///
    /// The field values are returned in a form that is compatible with
    /// conversion to SQL type, but pre-conversion.
    ///
    /// Note: Any fields using an intermediary type (i.e. specified with the
    /// `via` attribute argument) will be converted to that type before being
    /// returned.
    ///
    fn field_values(&self) -> Vec<Box<dyn ToSql + Send>>;

    /// Converts a row from the database into a record.
    ///
    /// This function is used to convert a row from the database from primitive
    /// SQL types into a Rust record type. It is used to convert the results of
    /// a query into a specific type `T`.
    ///
    /// # Parameters
    ///
    /// * `row`     - The row from the database to convert into a record.
    /// * `columns` - The names of the columns in the row.
    /// * `stash`   - The associated [`Stash`] instance for the operation.
    ///
    /// # Errors
    ///
    /// This function will return a [`ConversionError`] if there is a problem
    /// converting the row.
    ///
    fn from_row(row: &Row<'_>, columns: &[String]) -> Result<Self, ConversionError>;
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
pub trait Model: DbRecord
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
    /// * `params`      - The parameters to use in the query. These should be in
    ///   the order they are expected in the query logic, and
    ///   match with any expectations set in the query logic.
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///   to use for finding the records.
    /// * `queue`       - An optional queue to send changes to. If this is
    ///   provided, the function will listen for changes to the
    ///   result set and send them to the queue. This is useful
    ///   for live updates.
    fn find<Q>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> impl Future<Output = Result<Vec<Self>, StashError>> + Send
    where
        Q: Into<String> + Send,
    {
        async move { perform_find(query_logic, params, tether).await }
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
    /// * `params`      - The parameters to use in the query. These should be in
    ///   the order they are expected in the query logic, and
    ///   match with any expectations set in the query logic.
    /// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
    ///   to use for finding the records.
    ///
    /// # Errors
    ///
    /// See [`Stash::query()`].
    ///
    /// # See also
    ///
    /// * [`Model::find()`]
    /// * [`Model::load()`]
    /// * [`Stash::query()`]
    /// * [`params!`](crate::utils::params)
    ///
    fn find_first<Q>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> impl Future<Output = Result<Option<Self>, StashError>> + Send
    where
        Q: Into<String> + Send,
    {
        async move {
            Ok(
                perform_find(format!("{} LIMIT 1", query_logic.into()), params, tether)
                    .await?
                    .into_iter()
                    .next(),
            )
        }
    }

    /// Gets the record's local id.
    ///
    /// # Panics
    ///
    /// This function will panic if the local id has not been set.
    /// Normally you set it by calling
    /// [`Model::save`]
    fn id(&self) -> Self::IdType;

    /// Gets the name of the ID field for the record type.
    ///
    /// This is the primary key column name for the record as defined when
    /// creating the table.
    fn id_field_name() -> &'static str;

    /// Loads a record from the database by ID.
    ///
    /// This function retrieves a single record from the database by its unique
    /// ID. It is a convenience method for calling [`Stash::query()`] and then
    /// converting the first result to the desired type `T`.
    ///
    /// If no results are found, [`None`] will be returned. Having no results is
    /// not considered to be an error case.
    ///
    /// After loading, the [`Stash`] will be set against the record instance, so
    /// that instance-based operations have the correct context.
    ///
    /// # Parameters
    ///
    /// * `id`        - The ID of the record to load.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for loading the record. It is necessary to provide
    ///   this in order to know where to load the record from.
    ///
    /// # Errors
    ///
    /// See [`Stash::query()`] for a list of possible errors that can occur when
    /// using this function.
    ///
    /// # See also
    ///
    /// * [`Stash::load()`]
    /// * [`Tether::load()`]
    ///
    #[must_use]
    async fn load(id: Self::IdType, tether: &Tether) -> Result<Option<Self>, StashError> {
        perform_load(id, tether).await
    }

    /// Saves a record to the database, using a specific connection.
    ///
    /// This function saves a single record to the database by its unique ID. It
    /// is a convenience method for calling [`Stash::execute()`] and passing in
    /// the data.
    ///
    /// There are one prerequisite for calling this function:
    ///
    ///   1. The record must have a unique ID. This needs to have been set on
    ///      the record instance, or an error will occur.
    ///
    /// # Logic
    ///
    /// There are a number of factors that determine the approach taken to
    /// saving a record. The decisions to make are firstly whether to perform an
    /// `INSERT` or an `UPDATE`, and secondly whether to include the ID field in
    /// the query.
    ///
    /// The factors influencing the decision are: whether the row ID is set,
    /// whether the ID field is set, and whether the ID field has been
    /// configured as optional or auto-incrementing.
    ///
    /// If the ID field is auto-incrementing (optional and database-managed):
    ///
    ///   - Row ID set, ID field set: `UPDATE`
    ///   - Row ID set, ID field not set: [`StashError::InvalidIdState`]
    ///   - Row ID not set, ID field set: [`StashError::InvalidIdState`]
    ///   - Row ID not set, ID field not set: `INSERT`
    ///
    /// If the ID field is optional (but not auto-incrementing, i.e. manual):
    ///
    ///   - Row ID set, ID field set: `UPDATE`
    ///   - Row ID set, ID field not set: [`StashError::InvalidIdState`]
    ///   - Row ID not set, ID field set: `INSERT`
    ///   - Row ID not set, ID field not set: [`StashError::IdNotSet`]
    ///
    /// If the ID field is fully manual:
    ///
    ///   - Row ID set: `UPDATE`
    ///   - Row ID not set: `INSERT`
    ///
    /// Note: If the ID field is set to optional or auto-incrementing, then the
    /// [`id_value()`](Model::id_value()) function will return an error if it is
    /// not set, which is how we can determine this.
    ///
    /// # Errors
    ///
    /// See [`Stash::query()`] for a list of possible general query-related
    /// errors that can occur when using this function. In addition, the
    /// following may occur:
    ///
    /// * [`StashError::IdNotSet`]
    /// * [`StashError::InvalidIdState`]
    /// * [`StashError::NoRowIdReturned`]
    /// * [`StashError::NoRowsUpdated`]
    ///
    async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        perform_save(self, bond).await
    }

    /// Gets the name of the table for the record type.
    fn table_name() -> &'static str;

    /// Counts models in database.
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
    /// When querying the database fails.
    ///
    fn count<Q>(
        query_logic: Q,
        params: Vec<Box<dyn ToSql + Send>>,
        tether: &Tether,
    ) -> impl Future<Output = Result<u64, StashError>> + Send
    where
        Q: Into<String> + Send,
    {
        async move {
            tether
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
    }

    // The following methods are intended to be used within the ORM, generated from the proc macro.

    fn field_names_without_id() -> Vec<&'static str>;
    fn field_values_without_id(&self) -> Vec<Box<dyn ToSql + Send>>;
    fn id_is_autoincrementing() -> bool;
    fn id_is_optional() -> bool;
    fn id_value(&self) -> Result<Self::IdType, StashError>;
    fn row_id(&self) -> Option<u64>;
    fn set_id_value(&mut self, id: Self::IdType);
    fn set_row_id(&mut self, id: Option<u64>);
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

/// Converts [`Rows`] into a [`Vec`] of `T` record types.
///
/// This function is used to convert the results of a database query into a set
/// of records. It expects `T` to be a type that implements the [`DbRecord`]
/// trait and provides a [`from_row`](DbRecord::from_row()) method. This will be
/// called for each row in the query results to convert the row into a record.
/// The key point of this function is to provide contextual information in the
/// form of columns along with the row data.
///
/// # Parameters
///
/// * `rows`  - The query results to convert into records.
/// * `stash` - The associated [`Stash`] instance for the operation.
///
/// # Errors
///
/// This function will return a [`ConversionError`] if there is a problem
/// converting the row.
///
pub fn from_rows<T: DbRecord>(mut rows: Rows<'_>) -> Result<Vec<T>, ConversionError> {
    let columns = rows
        .as_ref()
        .map(|statement| {
            (0..statement.column_count())
                .map(|i| {
                    statement
                        .column_name(i)
                        .map(ToOwned::to_owned)
                        .map_err(|err| ConversionError::ColumnNameNotAvailable(i, err))
                })
                .collect::<Result<Vec<_>, ConversionError>>()
        })
        .ok_or(ConversionError::ColumnNamesNotAvailable)??;
    let mut results = vec![];
    while let Some(row) = rows.next()? {
        results.push(T::from_row(row, &columns)?);
    }
    Ok(results)
}

/// Finds records in the database using specific query logic.
///
/// This function carries out the actual finding logic, allowing the
/// [`Model::find()`] and [`Model::find_first()`] functions to call it, along
/// with any `on_load()` custom logic that may be required.
///
/// For full usage details, see [`Model::find()`].
///
/// # Parameters
///
/// * `query_logic` - The query logic to use for finding the records. This
///   should be a string that represents the conditions,
///   ordering, offset, and limit for the query, as may be
///   required. It can be empty. Note that each part of the
///   logic is optional — so if conditions are passed, for
///   instance, the `WHERE` keyword needs to be included.
/// * `params`      - The parameters to use in the query. These should be in the
///   order they are expected in the query logic, and match with
///   any expectations set in the query logic.
/// * `interface`   - The database interface, i.e. [`Stash`] or [`Tether`],
///   to use for finding the records.
/// * `queue`       - An optional queue to send changes to. If this is provided,
///   the function will listen for changes to the result set and
///   send them to the queue. This is useful for live updates.
///
/// # Errors
///
/// See [`Stash::query()`].
///
/// # See also
///
/// * [`Model::find()`]
/// * [`Model::find_first()`]
/// * [`Model::load()`]
/// * [`Stash::query()`]
/// * [`params!`](crate::utils::params)
///
pub async fn perform_find<Q, T>(
    query_logic: Q,
    params: Vec<Box<dyn ToSql + Send>>,
    tether: &Tether,
) -> Result<Vec<T>, StashError>
where
    Q: Into<String> + Send,
    T: Model,
{
    let query = formatdoc!(
        "
            SELECT
                {}.rowid AS rowid, *
            FROM
                {}
            {}
        ",
        T::table_name(),
        T::table_name(),
        query_logic.into(),
    );
    let records = tether.query(query, params).await?;

    Ok(records)
}

/// Loads a record from the database by ID.
///
/// This function retrieves a single record from the database by its unique ID,
/// as an instance of the specified type `T`, where `T` is any concrete type
/// implementing the [`Model`] trait. It is the internal function that actually
/// does the loading for the public interface methods [`Model::load()`],
/// [`Stash::load()`] and [`Tether::load()`].
///
/// For full usage details, see [`Model::load()`].
///
/// # Parameters
///
/// * `id`        - The ID of the record to load.
/// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
///   use for loading the record.
///
/// # Errors
///
/// See [`Model::load()`].
///
/// # See also
///
/// * [`Model::load()`]
/// * [`Stash::load()`]
/// * [`Tether::load()`]
///
pub async fn perform_load<T, I>(id: I, tether: &Tether) -> Result<Option<T>, StashError>
where
    T: Model,
    I: ToSql + Send + 'static,
{
    let query = formatdoc!(
        "
        SELECT
            {}.rowid AS rowid, *
        FROM
            {}
        WHERE
            {} = ?
        LIMIT
            1
    ",
        T::table_name(),
        T::table_name(),
        T::id_field_name(),
    );
    #[allow(trivial_casts)]
    Ok(tether
        .query::<_, T>(&query, vec![Box::new(id) as Box<dyn ToSql + Send>])
        .await?
        .into_iter()
        .next())
}

/// Saves a record to the database.
///
/// This function saves a single record to the database by its unique ID, either
/// ad-hoc or using a specific [`Tether`], i.e. connection. It is the internal
/// function that actually does the saving for the public interface method
/// [`save()`](Model::save())
///
/// For full usage details, see [`save()`](Model::save()).
///
/// # Parameters
///
/// * `model`     - The [`Model`] instance.
/// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to use
///   for saving the record.
///
/// # Errors
///
/// See [`Model::save()`].
///
/// # See also
///
/// * [`Model::save()`]
///
#[allow(clippy::too_many_lines)]
pub async fn perform_save<M: Model>(model: &mut M, bond: &Bond<'_>) -> Result<(), StashError> {
    // If the ID field is auto-incrementing then it is fully managed by the
    // database, and we exclude it from the list here.
    let (fields, values) = if M::id_is_autoincrementing() {
        (
            M::field_names_without_id(),
            M::field_values_without_id(model),
        )
    } else {
        (M::field_names(), M::field_values(model))
    };

    match (model.row_id(), model.id_value()) {
        // The row ID is set, but the optional ID field is not - invalid state.
        (Some(_), Err(_)) => {
            return Err(StashError::InvalidIdState);
        }
        // The row ID and the ID field are both set - perform an update.
        (Some(_), Ok(id)) => {
            let update_fields = fields.iter().map(|field| format!("{field} = ?")).join(", ");
            let query = formatdoc!(
                "
                UPDATE
                    {}
                SET
                    {}
                WHERE
                    {} = ?
            ",
                M::table_name(),
                update_fields,
                M::id_field_name(),
            );
            #[allow(trivial_casts)]
            let field_values: Vec<Box<dyn ToSql + Send>> = values
                .into_iter()
                .chain(once(Box::new(id) as Box<dyn ToSql + Send>))
                .collect();
            #[allow(clippy::shadow_reuse)]
            let affected: usize = bond.execute(&query, field_values).await?;

            if affected == 0 {
                return Err(StashError::NoRowsUpdated);
            }
        }
        // The row ID is not set (the ID field may or may not be set) - new record.
        (None, _) => {
            if M::id_is_autoincrementing() && model.id_value().is_ok() {
                // If the ID field is configured as auto-incrementing and is set, but the
                // row ID is not set, then the state is invalid, because it should have been
                // loaded from the database.
                return Err(StashError::InvalidIdState);
            }
            if M::id_is_optional() && !M::id_is_autoincrementing() && model.id_value().is_err() {
                // If the ID field is configured as optional but NOT auto-incrementing, and
                // is not set, then the state is invalid, because it is under manual control
                // and needs to be set before saving.
                return Err(StashError::IdNotSet);
            }
            let placeholders = crate::utils::placeholders(&fields);
            let query = formatdoc!(
                "
                INSERT INTO
                    {} ({})
                VALUES
                    ({})
                RETURNING
                    {}.rowid AS rowid, {} AS id
            ",
                M::table_name(),
                fields.join(", "),
                placeholders,
                M::table_name(),
                M::id_field_name(),
            );
            let field_values: Vec<Box<dyn ToSql + Send>> = values.into_iter().collect();
            #[allow(clippy::shadow_reuse)]
            let rows = bond
                .query::<_, QueryResultIdPair<M::IdType>>(&query, field_values)
                .await?;
            if let Some(row) = rows.into_iter().next() {
                model.set_id_value(row.id);
                model.set_row_id(Some(row.rowid));
            } else {
                return Err(StashError::NoRowIdReturned);
            }
        }
    };
    Ok(())
}
