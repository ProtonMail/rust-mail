//! Common data types used for common generic query responses.

use crate as stash;
use crate::macros::DbRecord;
use core::fmt::Debug;
use rusqlite::types::FromSql;
use rusqlite::ToSql;
use serde::{Deserialize, Serialize};

/// A query result that returns a boolean value.
#[derive(Copy, Clone, DbRecord, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct QueryResultBool {
    /// The value of the query result.
    #[DbField]
    pub value: bool,
}

/// A query result that returns a signed integer value.
#[derive(Copy, Clone, DbRecord, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct QueryResultI64 {
    /// The value of the query result.
    #[DbField]
    pub value: i64,
}

/// A query result that returns an ID field plus a row ID.
#[derive(Copy, Clone, DbRecord, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct QueryResultIdPair<I>
where
    I: Clone + Debug + FromSql + PartialEq + ToSql + Send + Sync + 'static,
{
    /// The ID field value.
    #[DbField]
    pub id: I,

    /// The internal row ID.
    #[DbField]
    pub rowid: u64,
}

/// A query result that returns a string value.
#[derive(Clone, DbRecord, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct QueryResultString {
    /// The value of the query result.
    #[DbField]
    pub value: String,
}

/// A query result that returns an unsigned integer value.
#[derive(Copy, Clone, DbRecord, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct QueryResultU64 {
    /// The value of the query result.
    #[DbField]
    pub value: u64,
}
