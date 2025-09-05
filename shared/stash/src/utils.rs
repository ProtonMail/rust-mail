//! Utility functions for working with SQLite.
//! This module provides utility functions for working with SQLite, including
//! functions for converting values to and from SQLite values using `serde`.
//!

use crate::stash::RusqliteResult;
use core::str::from_utf8;
use rusqlite::types::{FromSql, FromSqlError, ToSqlOutput, ValueRef};
use rusqlite::{Connection, Error as SqliteError, Params, ToSql};
use serde::{Deserialize, Serialize};
use serde_json::{from_str as from_json, to_string as to_json};
use std::borrow::Cow;
use tracing::warn;

/// Boxes up a list of types, making them suitable for use as query parameters.
///
/// This is a convenience macro, allowing the creation of a boxed vector of
/// types to be shortened. Instead of wrapping each parameter in `Box::new()`,
/// they can instead be passed bare to this macro, which will box them and
/// return a [`Vec`] of boxed parameters.
///
#[macro_export]
macro_rules! params {
    ($($param:expr),+) => {
        vec![$(Box::new($param) as _),+]
    };
}

fn boxed(x: impl ToSql + Send + 'static) -> Box<dyn ToSql + Send> {
    Box::new(x)
}

pub trait IterMapToSql
where
    Self: Sized,
{
    fn bridge_sql(self) -> Vec<Box<dyn ToSql + Send>> {
        self.bridge_sql_iter().collect()
    }

    fn bridge_sql_extend_iter(
        self,
        other: impl IterMapToSql,
    ) -> impl Iterator<Item = Box<dyn ToSql + Send>> {
        self.bridge_sql_iter().chain(other.bridge_sql_iter())
    }

    fn bridge_sql_extend(self, other: impl IterMapToSql) -> Vec<Box<dyn ToSql + Send>> {
        self.bridge_sql_iter()
            .chain(other.bridge_sql_iter())
            .collect()
    }

    fn bridge_sql_iter(self) -> impl Iterator<Item = Box<dyn ToSql + Send>>;
}

impl<T: ToSql + Send + 'static, I: IntoIterator<Item = T>> IterMapToSql for I {
    fn bridge_sql_iter(self) -> impl Iterator<Item = Box<dyn ToSql + Send>> {
        self.into_iter().map(boxed)
    }
}

pub trait MapToSql
where
    Self: Sized,
{
    fn to_iter_map_to_sql(self) -> impl IterMapToSql;

    fn to_sql_iter(self) -> impl Iterator<Item = Box<dyn ToSql + Send>> {
        self.to_iter_map_to_sql().bridge_sql_iter()
    }

    fn to_sql(self) -> Vec<Box<dyn ToSql + Send>> {
        self.to_iter_map_to_sql().bridge_sql_iter().collect()
    }

    fn to_sql_extend_iter(
        self,
        other: impl MapToSql,
    ) -> impl Iterator<Item = Box<dyn ToSql + Send>> {
        self.to_sql_iter().chain(other.to_sql_iter())
    }

    fn to_sql_extend(self, other: impl MapToSql) -> Vec<Box<dyn ToSql + Send>> {
        self.to_sql_extend_iter(other).collect()
    }
}

impl<T: Clone + ToSql + Send + 'static> MapToSql for &[T] {
    fn to_iter_map_to_sql(self) -> impl IterMapToSql {
        self.iter().cloned()
    }
}

impl<T1> MapToSql for (T1,)
where
    T1: ToSql + Send + 'static,
{
    fn to_iter_map_to_sql(self) -> impl IterMapToSql {
        [boxed(self.0)].into_iter()
    }
}

impl<T1, T2> MapToSql for (T1, T2)
where
    T1: ToSql + Send + 'static,
    T2: ToSql + Send + 'static,
{
    fn to_iter_map_to_sql(self) -> impl IterMapToSql {
        [boxed(self.0), boxed(self.1)].into_iter()
    }
}

impl<T1, T2, T3> MapToSql for (T1, T2, T3)
where
    T1: ToSql + Send + 'static,
    T2: ToSql + Send + 'static,
    T3: ToSql + Send + 'static,
{
    fn to_iter_map_to_sql(self) -> impl IterMapToSql {
        [boxed(self.0), boxed(self.1), boxed(self.2)].into_iter()
    }
}

pub use params;

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

#[must_use]
pub fn placeholders<_T>(input: &[_T]) -> Cow<'static, str> {
    placeholders_n(input.len())
}

/// Use this when you need to create a string with a known number of placeholders in the form
/// "?,?,?,?"
#[must_use]
pub fn placeholders_n(n: usize) -> Cow<'static, str> {
    /// This has 100 placeholders
    static PLACEHOLDERS: &str = "?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?";
    if n < 100 {
        #[allow(clippy::string_slice)]
        #[allow(clippy::arithmetic_side_effects)]
        Cow::Borrowed(&PLACEHOLDERS[..(n * 2).saturating_sub(1)])
    } else {
        /// This has much better codegen.
        /// We mark this branch as unlikely and move the logic into a different
        /// function that can't get inlined for better icache.
        #[cold]
        #[inline(never)]
        fn cold(n: usize) -> Cow<'static, str> {
            warn!("Too many placeholders! please increase the static placeholder.");
            let mut res = "?,".repeat(n);
            _ = res.pop();
            Cow::Owned(res)
        }
        cold(n)
    }
}

pub trait ConnectionExt {
    fn query_rows_col<T: FromSql>(
        &self,
        sql: impl AsRef<str>,
        params: impl Params,
    ) -> RusqliteResult<Vec<T>>;
    fn query_row_col<T: FromSql>(
        &self,
        sql: impl AsRef<str>,
        params: impl Params,
    ) -> RusqliteResult<T>;
    fn query_row_col_2<T: FromSql, U: FromSql>(
        &self,
        sql: impl AsRef<str>,
        params: impl Params,
    ) -> RusqliteResult<(T, U)>;
}

impl ConnectionExt for Connection {
    fn query_rows_col<T: FromSql>(
        &self,
        sql: impl AsRef<str>,
        params: impl Params,
    ) -> RusqliteResult<Vec<T>> {
        let mut stmt = self.prepare(sql.as_ref())?;
        stmt
            .query_map(params, |x| x.get::<_, T>(0))?
            .collect::<Result<_, _>>()
    }

    fn query_row_col<T: FromSql>(
        &self,
        sql: impl AsRef<str>,
        params: impl Params,
    ) -> RusqliteResult<T> {
        let mut stmt = self.prepare(sql.as_ref())?;
        stmt.query_row(params, |x| x.get(0))
    }

    fn query_row_col_2<T: FromSql, U: FromSql>(
        &self,
        sql: impl AsRef<str>,
        params: impl Params,
    ) -> RusqliteResult<(T, U)> {
        let mut stmt = self.prepare(sql.as_ref())?;
        stmt.query_row(params, |x| Ok((x.get(0)?, x.get(1)?)))
    }
}
