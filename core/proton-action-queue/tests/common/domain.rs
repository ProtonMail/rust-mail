use core::fmt;
use std::fmt::Display;
use std::num::ParseIntError;
use std::str::FromStr;
use proton_sqlite3::rusqlite;
use proton_sqlite3::rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use stash::macros::Model;
use stash::orm::CsvArray;
use stash::stash::Stash;

#[derive(Debug, Clone, Ord, PartialOrd, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageId(pub u32);

impl FromSql for MessageId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u32::column_result(value).map(|v| MessageId(v))
    }
}

impl ToSql for MessageId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[derive(Debug, Clone, PartialOrd, Ord, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct FolderId(pub u32);

impl FromSql for FolderId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u32::column_result(value).map(|v| FolderId(v))
    }
}

impl ToSql for FolderId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[derive(Debug, Clone, PartialOrd, Ord, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct LabelId(pub u32);

impl Display for LabelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for LabelId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u32>().map(LabelId)
    }
}

impl FromSql for LabelId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        u32::column_result(value).map(LabelId)
    }
}

impl ToSql for LabelId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[TableName("messages")]
pub struct Message {
    #[IdField]
    pub id: MessageId,
    #[DbField]
    pub folder: Option<FolderId>,
    #[DbField(via CsvArray<LabelId>)]
    pub labels: Vec<LabelId>,
    #[DbField]
    pub read: bool,
    #[RowIdField]
    #[serde(skip)]
    row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    stash: Option<Stash>,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Label {
    pub id: LabelId,
    pub name: String,
}
