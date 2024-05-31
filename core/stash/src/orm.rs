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

use crate::stash::{Stash, StashError, Tether};
use core::any::Any;
use core::fmt::{self, Debug, Display};
use core::iter::repeat;
use core::str::FromStr;
use indoc::formatdoc;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef};
use rusqlite::{Error as SqliteError, Row, Rows, ToSql};
use serde::de::Error as DeserializationError;
use serde::ser::Error as SerializationError;
use serde::{Deserialize, Serialize};
use serde_json::{from_str as from_json, to_string as to_json};
use std::collections::HashMap;
use std::error::Error;
use std::vec::IntoIter;

/// Errors for conversion of database row data into record types.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum ConversionError {
    /// For some reason it is not possible to obtain a name for a particular
    /// column. This refers specifically to trying to obtain the information
    /// from the database query results, and technically should never happen, as
    /// it would mean there is a column present in the resultset without a name.
    ColumnNameNotAvailable(usize, SqliteError),

    /// For some reason it is not possible to obtain column names. This refers
    /// specifically to trying to obtain the information from the database query
    /// results.
    ColumnNamesNotAvailable,

    /// Basic deserialisation error from [`serde`].
    DeserializationError(Option<String>, String),

    /// The row data returned from the database query is missing a column
    /// according to the expectations of the record type.
    MissingColumn(String),

    /// SQL-related error from [`rusqlite`].
    SqliteError(SqliteError),

    /// Basic serialisation error from [`serde`].
    SerializationError(String),
}

impl DeserializationError for ConversionError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::DeserializationError(None, msg.to_string())
    }
}

impl Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match *self {
            Self::ColumnNameNotAvailable(i, ref error) => {
                write!(f, "Column {i}'s name is not available: {error}")
            }
            Self::ColumnNamesNotAvailable => write!(f, "Column names are not available"),
            Self::DeserializationError(None, ref message) => {
                write!(f, "Deserialization error: {message}")
            }
            Self::DeserializationError(Some(ref column), ref message) => write!(
                f,
                r#"Deserialization error for column "{column}": {message}"#
            ),
            Self::MissingColumn(ref column) => write!(f, r#"Missing column: "{column}""#),
            Self::SqliteError(ref error) => write!(f, "SQLite error: {error}"),
            Self::SerializationError(ref message) => write!(f, "Serialization error: {message}"),
        }
    }
}

impl Error for ConversionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            Self::SqliteError(ref err) => Some(err),
            Self::ColumnNameNotAvailable(..)
            | Self::ColumnNamesNotAvailable
            | Self::DeserializationError(..)
            | Self::MissingColumn(_)
            | Self::SerializationError(_) => None,
        }
    }
}

impl From<SqliteError> for ConversionError {
    fn from(err: SqliteError) -> Self {
        Self::SqliteError(err)
    }
}

impl SerializationError for ConversionError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::SerializationError(msg.to_string())
    }
}

/// Wrapper type to represent an array of CSV values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct CsvArray<T>(Vec<T>);

impl<T> From<CsvArray<T>> for Vec<T> {
    fn from(value: CsvArray<T>) -> Self {
        value.0
    }
}

impl<T> From<Vec<T>> for CsvArray<T> {
    fn from(vec: Vec<T>) -> Self {
        Self(vec)
    }
}

impl<T: FromStr> FromSql for CsvArray<T>
where
    T::Err: Debug + Error + Send + Sync + 'static,
{
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Null => Ok(Self(vec![])),
            ValueRef::Blob(_) | ValueRef::Integer(_) | ValueRef::Real(_) | ValueRef::Text(_) => {
                value
                    .as_str()?
                    .split(',')
                    .map(|str| {
                        str.parse()
                            .map_err(|err| FromSqlError::Other(Box::new(err)))
                    })
                    .collect::<Result<Vec<T>, _>>()
                    .map(CsvArray)
            }
        }
    }
}

impl<T: ToString> ToSql for CsvArray<T> {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        if self.0.is_empty() {
            return Ok(ToSqlOutput::Owned(Value::Null));
        }
        Ok(ToSqlOutput::from(
            self.0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join(","),
        ))
    }
}

/// Wrapper type to represent an array of JSON values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct JsonArray<T>(Vec<T>);

impl<T> From<JsonArray<T>> for Vec<T> {
    fn from(value: JsonArray<T>) -> Self {
        value.0
    }
}

impl<T> From<Vec<T>> for JsonArray<T> {
    fn from(vec: Vec<T>) -> Self {
        Self(vec)
    }
}

impl<T: for<'de> Deserialize<'de>> FromSql for JsonArray<T> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(Self(match value {
            ValueRef::Null => vec![],
            ValueRef::Blob(_) | ValueRef::Integer(_) | ValueRef::Real(_) | ValueRef::Text(_) => {
                from_json(value.as_str()?).map_err(|err| FromSqlError::Other(Box::new(err)))?
            }
        }))
    }
}

impl<T: Serialize> ToSql for JsonArray<T> {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        if self.0.is_empty() {
            return Ok(ToSqlOutput::Owned(Value::Null));
        }
        Ok(ToSqlOutput::from(to_json(&self.0).map_err(|err| {
            SqliteError::ToSqlConversionFailure(Box::new(err))
        })?))
    }
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
    /// Gets a list of fields with names and associated values for the record.
    ///
    /// The field values are returned in a form that is compatible with
    /// conversion to SQL type, but pre-conversion.
    ///
    /// Note: Any fields using an intermediary type (i.e. specified with the
    /// `via` attribute argument) will be converted to that type before being
    /// returned.
    ///
    fn fields(&self) -> HashMap<&'static str, Box<dyn ToSql + Send>>;

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
    fn from_row(row: &Row<'_>, columns: &[String], stash: Stash) -> Result<Self, ConversionError>;
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
    /// The ID type for the record.
    type Id: ToSql;

    /// Gets the record's unique ID.
    fn id(&self) -> Self::Id;

    /// Gets the name of the ID field for the record type.
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
    /// * `id`    - The ID of the record to load.
    /// * `stash` - The database, i.e. [`Stash`], to use for loading the record.
    ///             It is necessary to provide this in order to know where to
    ///             load the record from.
    ///
    /// # Errors
    ///
    /// See [`Stash::query()`] for a list of possible errors that can occur when
    /// using this function.
    ///
    /// # See also
    ///
    /// * [`Model::load_using()`]
    /// * [`Stash::load()`]
    /// * [`Tether::load()`]
    ///
    #[must_use]
    async fn load(id: Self::Id, stash: &Stash) -> Result<Option<Self>, StashError> {
        stash.load(id).await
    }

    /// Loads a record from the database by ID, using a specific connection.
    ///
    /// This function retrieves a single record from the database by its unique
    /// ID, using a specific [`Tether`], i.e. connection. It is functionally
    /// equivalent to [`load()`](Model::load()), but allows the query to be run
    /// against an existing connection rather than using a new one.
    ///
    /// For full usage details, see [`load()`](Model::load()).
    ///
    /// Note that the [`Tether`] used will not be stored.
    ///
    /// # Parameters
    ///
    /// * `id`     - The ID of the record to load.
    /// * `tether` - The database connection, i.e. [`Tether`], to use for
    ///              loading the record. This allows an existing connection to
    ///              be used, rather than creating a new one.
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
    #[must_use]
    async fn load_using(id: Self::Id, tether: &Tether) -> Result<Option<Self>, StashError> {
        tether.load(id).await
    }

    /// Saves a record to the database.
    ///
    /// This function saves a single record to the database by its unique ID. It
    /// is a convenience method for calling [`Stash::execute()`] and passing in
    /// the data.
    ///
    /// There are two prerequisites for calling this function:
    ///
    ///   1. The record must have a unique ID. This needs to have been set on
    ///      the record instance, or an error will occur.
    ///   2. The [`Stash`] must be set on the record instance. This is necessary
    ///      to know where to save the record to.
    ///
    /// # Errors
    ///
    /// See [`Stash::query()`] for a list of possible errors that can occur when
    /// using this function.
    ///
    /// # See also
    ///
    /// * [`Model::save_using()`]
    ///
    async fn save(&self) -> Result<(), StashError> {
        let fields = Self::field_names();
        let placeholders = repeat("?")
            .take(fields.len())
            .collect::<Vec<_>>()
            .join(", ");
        let update_fields = fields
            .iter()
            .map(|field| format!("{field} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let query = formatdoc!(
            "
            INSERT INTO
                {} ({})
            VALUES
                ({})
            ON CONFLICT ({}) DO UPDATE SET {}
        ",
            Self::table_name(),
            fields.join(", "),
            placeholders,
            Self::id_field_name(),
            update_fields,
        );
        let _: usize = self
            .stash()
            .execute(
                &query,
                Self::field_values(self)
                    .into_iter()
                    .chain(Self::field_values(self))
                    .collect(),
            )
            .await?;
        Ok(())
    }

    /// Saves a record to the database, using a specific connection.
    ///
    /// This function saves a single record to the database by its unique ID,
    /// using a specific [`Tether`], i.e. connection. It is functionally
    /// equivalent to [`save()`](Model::save()), but allows the query to be run
    /// against an existing connection rather than using a new one.
    ///
    /// For full usage details, see [`save()`](Model::save()).
    ///
    /// Note that the [`Tether`] used will not be stored.
    ///
    /// # Parameters
    ///
    /// * `tether` - The database connection, i.e. [`Tether`], to use for
    ///              loading the record. This allows an existing connection to
    ///              be used, rather than creating a new one.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    /// # See also
    ///
    /// * [`Model::save()`]
    ///
    async fn save_using(&self, tether: &Tether) -> Result<(), StashError> {
        let fields = Self::field_names();
        let placeholders = repeat("?")
            .take(fields.len())
            .collect::<Vec<_>>()
            .join(", ");
        let update_fields = fields
            .iter()
            .map(|field| format!("{field} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let query = formatdoc!(
            "
            INSERT INTO
                {} ({})
            VALUES
                ({})
            ON CONFLICT ({}) DO UPDATE SET {}
        ",
            Self::table_name(),
            fields.join(", "),
            placeholders,
            Self::id_field_name(),
            update_fields,
        );
        let _: usize = tether
            .execute(
                &query,
                Self::field_values(self)
                    .into_iter()
                    .chain(Self::field_values(self))
                    .collect(),
            )
            .await?;
        Ok(())
    }

    /// Gets a reference to the database-handling [`Stash`] for the record.
    fn stash(&self) -> &Stash;

    /// Sets the database-handling [`Stash`] for the record.
    ///
    /// # Parameters
    ///
    /// * `stash` - The [`Stash`] to set for the record.
    ///
    fn set_stash(&mut self, stash: &Stash);

    /// Gets the name of the table for the record type.
    fn table_name() -> &'static str;
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
pub struct DbRecords(Vec<Box<dyn Any + Send + 'static>>);

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
pub fn from_rows<T: DbRecord>(
    mut rows: Rows<'_>,
    stash: &Stash,
) -> Result<Vec<T>, ConversionError> {
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
        results.push(T::from_row(row, &columns, stash.clone())?);
    }
    Ok(results)
}
