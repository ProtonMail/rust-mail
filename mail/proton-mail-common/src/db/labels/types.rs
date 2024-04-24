use crate::db::ids::new_u64_type;
use crate::db::labels::movable_sys_folder_list;
use proton_api_mail::domain::{Label, LabelId, LabelType};
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};
use proton_sqlite3::rusqlite::types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef};
use proton_sqlite3::rusqlite::ToSql;

new_u64_type!(LocalLabelId);
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LocalLabel {
    pub id: LocalLabelId,
    pub rid: Option<LabelId>,
    pub parent_id: Option<LocalLabelId>,
    pub name: String,
    pub path: Option<String>,
    pub color: LabelColor,
    pub label_type: LabelType,
    pub order: u32,
    pub notify: bool,
    pub expanded: bool,
    pub sticky: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LocalLabelWithCount {
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
    pub total_count: u64,
    pub unread_count: u64,
}

impl LocalLabel {
    pub fn from_label(id: LocalLabelId, parent_id: Option<LocalLabelId>, label: Label) -> Self {
        Self {
            id,
            rid: Some(label.id),
            parent_id,
            name: label.name,
            path: label.path,
            color: LabelColor::from(label.color),
            label_type: label.label_type,
            order: label.order,
            notify: label.notify,
            expanded: label.expanded,
            sticky: label.sticky,
        }
    }

    pub fn is_movable_folder(&self) -> bool {
        self.label_type == LabelType::Folder
            || self
                .rid
                .as_ref()
                .map_or(false, |rid| movable_sys_folder_list().contains(&rid))
    }

    /// Check whether this label is a "labelable" label. This includes all labels of type `Label`
    /// and the Starred system label.
    pub fn is_applicable_label(&self) -> bool {
        self.label_type == LabelType::Label
            || self
                .rid
                .as_ref()
                .map_or(false, |rid| rid == LabelId::starred())
    }
}

impl From<LocalLabelWithCount> for LocalLabel {
    fn from(value: LocalLabelWithCount) -> Self {
        Self {
            id: value.id,
            rid: value.rid,
            parent_id: value.parent_id,
            name: value.name,
            path: value.path,
            color: value.color,
            label_type: value.label_type,
            order: value.order,
            notify: value.notified,
            expanded: value.expanded,
            sticky: value.sticky,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(crate = "self::serde")]
pub struct LabelColor(String);

#[cfg(feature = "uniffi")]
uniffi::custom_newtype!(LabelColor, String);

impl LabelColor {
    pub fn purple() -> Self {
        Self("#8080FF".into())
    }

    pub fn black() -> Self {
        Self("#000000".into())
    }
}

impl<T: Into<String>> From<T> for LabelColor {
    fn from(value: T) -> Self {
        Self(value.into())
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
