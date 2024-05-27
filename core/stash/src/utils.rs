//! Utility functions for working with SQLite.
//!
//! This module provides utility functions for working with SQLite, including
//! functions for converting values to and from SQLite values using `serde`.
//!

use core::str::from_utf8;
use rusqlite::types::{FromSqlError, ToSqlOutput, ValueRef};
use rusqlite::Error as SqliteError;
use serde::{Deserialize, Serialize};
use serde_json::{from_str as from_json, to_string as to_json};

/// Implements [`ToSql`](rusqlite::types::ToSql) and [`FromSql`](rusqlite::types::FromSql)
/// for a type using [`serde`].
///
/// This macro is a convenience macro to implement [`ToSql`](rusqlite::types::ToSql)
/// and [`FromSql`](rusqlite::types::FromSql) for a type using [`serde`]'s
/// [`Serialize`] and [`Deserialize`].
///
/// In a situation where [`ToSql`](rusqlite::types::ToSql) and [`FromSql`](rusqlite::types::FromSql)
/// are needed, and the database representation of the type is the same as the
/// general serialized form, this can be leveraged by implementing [`Serialize`]
/// and [`Deserialize`] for the type and then using this macro to automatically
/// implement [`ToSql`](rusqlite::types::ToSql) and [`FromSql`](rusqlite::types::FromSql)
/// to call those conversions.
///
#[macro_export]
macro_rules! sql_using_serde {
    ($t:ty) => {
        impl stash::exports::ToSql for $t {
            fn to_sql(&self) -> Result<stash::exports::ToSqlOutput, stash::exports::SqliteError> {
                stash::utils::to_sql_using_serialize(self)
            }
        }
        impl stash::exports::FromSql for $t {
            fn column_result(
                value: stash::exports::ValueRef,
            ) -> stash::exports::FromSqlResult<Self> {
                stash::utils::from_sql_using_deserialize(value)
            }
        }
    };
}

pub use sql_using_serde;

/// Convert a value to a SQLite value using [`serde_json`].
///
/// This function converts a value to a SQLite value using [`serde_json`]. The
/// type is expected to be text, and serialisable to JSON.
///
/// # Parameters
///
/// * `value` - The value to convert to a SQLite value by serialising it to
///             JSON.
///
/// # Errors
///
/// This function will return a [`SqliteError::ToSqlConversionFailure`] if the
/// value cannot be serialised to JSON.
///
pub fn to_sql_using_serialize<T: Serialize>(value: &T) -> Result<ToSqlOutput<'_>, SqliteError> {
    Ok(ToSqlOutput::from(to_json(value).map_err(|err| {
        SqliteError::ToSqlConversionFailure(Box::new(err))
    })?))
}

/// Convert a SQLite value to a value using [`serde_json`].
///
/// This function converts a SQLite value to a value using [`serde_json`]. The
// /// type is expected to be text, and serialisable to JSON.
///
/// # Parameters
///
/// * `value` - The SQLite value to convert to a value by deserialising it from
///             JSON.
///
/// # Errors
///
/// This function will return a [`FromSqlError::InvalidType`] if the value is
/// not a text value, and a [`FromSqlError::Other`] if the value cannot be
/// deserialised from JSON.
///
pub fn from_sql_using_deserialize<T: for<'de> Deserialize<'de>>(
    value: ValueRef<'_>,
) -> Result<T, FromSqlError> {
    match value {
        ValueRef::Text(text) => Ok(from_json(
            from_utf8(text).map_err(|err| FromSqlError::Other(Box::new(err)))?,
        )
        .map_err(|err| FromSqlError::Other(Box::new(err)))?),
        ValueRef::Blob(_) | ValueRef::Integer(_) | ValueRef::Null | ValueRef::Real(_) => {
            Err(FromSqlError::InvalidType)
        }
    }
}
