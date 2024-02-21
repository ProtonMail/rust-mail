use proton_api_mail::domain::{Label, LabelId, LabelType};
use proton_sqlite3::rusqlite::types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef};
use proton_sqlite3::rusqlite::ToSql;

use crate::ids::new_u64_type;

new_u64_type!(LocalLabelId);
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalLabel {
    pub id: LocalLabelId,
    pub rid: Option<LabelId>,
    pub parent_id: Option<LocalLabelId>,
    pub name: String,
    pub path: Option<String>,
    pub color: LabelColor,
    pub label_type: LabelType,
    pub order: u32,
    pub notified: bool,
    pub expanded: bool,
    pub sticky: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RemoteLabel {
    pub id: LabelId,
    pub parent_id: Option<LabelId>,
    pub name: String,
    pub path: Option<String>,
    pub label_type: LabelType,
    pub color: LabelColor,
    pub order: u32,
    pub notified: bool,
    pub expanded: bool,
    pub sticky: bool,
}
impl From<Label> for RemoteLabel {
    fn from(value: Label) -> Self {
        Self {
            id: value.id,
            parent_id: value.parent_id,
            name: value.name,
            path: value.path,
            label_type: value.label_type,
            color: LabelColor(value.color),
            order: value.order,
            notified: value.notify.into(),
            expanded: value.expanded.into(),
            sticky: value.sticky.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LabelColor(String);

impl LabelColor {
    pub fn purple() -> Self {
        Self("#8080FF".into())
    }

    pub fn black() -> Self {
        Self("#000000".into())
    }
}
impl AsRef<str> for LabelColor {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl ToSql for LabelColor {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

impl FromSql for LabelColor {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        String::column_result(value).map(Self)
    }
}
