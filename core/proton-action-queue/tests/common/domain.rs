use proton_sqlite3::rusqlite;
use proton_sqlite3::rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Message {
    pub id: MessageId,
    pub folder: Option<FolderId>,
    pub labels: Vec<LabelId>,
    pub read: bool,
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
