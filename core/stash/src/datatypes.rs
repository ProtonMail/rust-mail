//! Common data types used for common generic query responses.

use crate as stash;
use crate::macros::DbRecord;
use serde::{Deserialize, Serialize};

/// A query result that returns a signed integer value.
#[derive(Copy, Clone, DbRecord, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct QueryResultI64 {
    /// The value of the query result.
    #[DbField]
    pub value: i64,
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
