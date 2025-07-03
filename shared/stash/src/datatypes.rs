//! Common data types used for common generic query responses.

use crate::macros::DbRecord;
use crate::{self as stash};
use core::fmt::Debug;
use rusqlite::ToSql;
use rusqlite::types::FromSql;
use serde::{Deserialize, Serialize};

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
