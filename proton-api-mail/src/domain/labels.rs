use proton_api_core::domain::ProtonBoolean;
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::exports::serde_repr::{Deserialize_repr, Serialize_repr};

proton_api_core::utils::string_id!(LabelId);

#[derive(Debug, Deserialize_repr, Serialize_repr, Eq, PartialEq, Copy, Clone)]
#[repr(u8)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum LabelType {
    Label = 1,
    ContactGroup = 2,
    Folder = 3,
    System = 4,
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Label {
    #[serde(rename = "ID")]
    pub id: LabelId,
    #[serde(rename = "ParentID")]
    pub parent_id: Option<LabelId>,
    pub name: String,
    pub path: Option<String>,
    pub color: String,
    #[serde(rename = "Type")]
    pub label_type: LabelType,
    #[serde(default)]
    pub notify: ProtonBoolean,
    #[serde(default)]
    pub display: ProtonBoolean,
    #[serde(default)]
    pub sticky: ProtonBoolean,
    #[serde(default)]
    pub expanded: ProtonBoolean,
    #[serde(default = "default_label_order")]
    pub order: u32,
}

fn default_label_order() -> u32 {
    0
}

/// SysLabelID represents system label identifiers that are constant for every account.
#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub struct SysLabelId(&'static str);

impl PartialEq<LabelId> for SysLabelId {
    fn eq(&self, other: &LabelId) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<SysLabelId> for LabelId {
    fn eq(&self, other: &SysLabelId) -> bool {
        self.0 == other.0
    }
}

impl From<SysLabelId> for LabelId {
    fn from(value: SysLabelId) -> Self {
        Self(value.0.into())
    }
}

impl SysLabelId {
    pub const INBOX: SysLabelId = SysLabelId("0");
    pub const ALL_DRAFTS: SysLabelId = SysLabelId("1");
    pub const ALL_SENT: SysLabelId = SysLabelId("1");
    pub const TRASH: SysLabelId = SysLabelId("3");
    pub const SPAM: SysLabelId = SysLabelId("4");
    pub const ALL_MAIL: SysLabelId = SysLabelId("5");
    pub const ARCHIVE: SysLabelId = SysLabelId("5");
    pub const SENT: SysLabelId = SysLabelId("7");
    pub const DRAFTS: SysLabelId = SysLabelId("8");
    pub const OUTBOX: SysLabelId = SysLabelId("9");
    pub const STARRED: SysLabelId = SysLabelId("10");
    pub const ALL_SCHEDULED: SysLabelId = SysLabelId("12");
}

impl LabelId {
    pub fn inbox() -> Self {
        SysLabelId::INBOX.into()
    }

    pub fn all_drafts() -> Self {
        SysLabelId::ALL_DRAFTS.into()
    }

    pub fn all_sent() -> Self {
        SysLabelId::ALL_SENT.into()
    }

    pub fn trash() -> Self {
        SysLabelId::TRASH.into()
    }

    pub fn spam() -> Self {
        SysLabelId::SPAM.into()
    }

    pub fn all_mail() -> Self {
        SysLabelId::ALL_MAIL.into()
    }

    pub fn archive() -> Self {
        SysLabelId::ARCHIVE.into()
    }

    pub fn sent() -> Self {
        SysLabelId::SENT.into()
    }

    pub fn drafts() -> Self {
        SysLabelId::DRAFTS.into()
    }

    pub fn outbox() -> Self {
        SysLabelId::OUTBOX.into()
    }

    pub fn starred() -> Self {
        SysLabelId::STARRED.into()
    }

    pub fn all_scheduled() -> Self {
        SysLabelId::ALL_SCHEDULED.into()
    }
}

impl std::fmt::Display for SysLabelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "sql")]
use proton_api_core::exports::proton_sqlite3::rusqlite;

#[cfg(feature = "sql")]
impl rusqlite::types::FromSql for LabelType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match u8::column_result(value)? {
            1 => Ok(LabelType::Label),
            2 => Ok(LabelType::ContactGroup),
            3 => Ok(LabelType::Folder),
            4 => Ok(LabelType::System),
            v => Err(rusqlite::types::FromSqlError::OutOfRange(v as i64)),
        }
    }
}

#[cfg(feature = "sql")]
impl rusqlite::types::ToSql for LabelType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Owned(
            rusqlite::types::Value::Integer(*self as i64),
        ))
    }
}
