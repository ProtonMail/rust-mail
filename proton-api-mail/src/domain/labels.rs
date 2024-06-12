use lazy_static::lazy_static;
use proton_api_core::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::exports::serde_repr::{Deserialize_repr, Serialize_repr};
use proton_api_core::utils::{bool_from_integer, bool_to_integer};
use std::convert::Into;

proton_api_core::utils::string_id!(LabelId);

#[derive(Debug, Deserialize_repr, Serialize_repr, Eq, PartialEq, Copy, Clone, Hash)]
#[repr(u8)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum LabelType {
    Label = 1,
    ContactGroup = 2,
    Folder = 3,
    System = 4,
}

pub const ALL_LABEL_TYPES: [LabelType; 4] = [
    LabelType::Label,
    LabelType::ContactGroup,
    LabelType::Folder,
    LabelType::System,
];

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[allow(clippy::struct_excessive_bools)]
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
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub notify: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub display: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub sticky: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub expanded: bool,
    #[serde(default = "default_label_order")]
    pub order: u32,
}

fn default_label_order() -> u32 {
    0
}

/// `SysLabelID` represents system label identifiers that are constant for every account.
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
    pub const ARCHIVE: SysLabelId = SysLabelId("6");
    pub const SENT: SysLabelId = SysLabelId("7");
    pub const DRAFTS: SysLabelId = SysLabelId("8");
    pub const OUTBOX: SysLabelId = SysLabelId("9");
    pub const STARRED: SysLabelId = SysLabelId("10");
    pub const ALL_SCHEDULED: SysLabelId = SysLabelId("12");
    pub const ALMOST_ALL_MAIL: SysLabelId = SysLabelId("15");
}

lazy_static! {
    static ref LABEL_ID_INBOX: LabelId = SysLabelId::INBOX.into();
    static ref LABEL_ID_ALL_DRAFTS: LabelId = SysLabelId::ALL_DRAFTS.into();
    static ref LABEL_ID_ALL_SENT: LabelId = SysLabelId::ALL_SENT.into();
    static ref LABEL_ID_TRASH: LabelId = SysLabelId::TRASH.into();
    static ref LABEL_ID_SPAM: LabelId = SysLabelId::SPAM.into();
    static ref LABEL_ID_ALL_MAIL: LabelId = SysLabelId::ALL_MAIL.into();
    static ref LABEL_ID_ARCHIVE: LabelId = SysLabelId::ARCHIVE.into();
    static ref LABEL_ID_SENT: LabelId = SysLabelId::SENT.into();
    static ref LABEL_ID_DRAFTS: LabelId = SysLabelId::DRAFTS.into();
    static ref LABEL_ID_OUTBOX: LabelId = SysLabelId::OUTBOX.into();
    static ref LABEL_ID_STARRED: LabelId = SysLabelId::STARRED.into();
    static ref LABEL_ID_ALL_SCHEDULED: LabelId = SysLabelId::ALL_SCHEDULED.into();
    static ref LABEL_ID_ALMOST_ALL_MAIL: LabelId = SysLabelId::ALMOST_ALL_MAIL.into();
}

impl LabelId {
    #[must_use]
    pub fn inbox() -> &'static Self {
        &LABEL_ID_INBOX
    }

    #[must_use]
    pub fn all_drafts() -> &'static Self {
        &LABEL_ID_ALL_DRAFTS
    }

    #[must_use]
    pub fn all_sent() -> &'static Self {
        &LABEL_ID_ALL_SENT
    }

    #[must_use]
    pub fn trash() -> &'static Self {
        &LABEL_ID_TRASH
    }

    #[must_use]
    pub fn spam() -> &'static Self {
        &LABEL_ID_SPAM
    }

    #[must_use]
    pub fn all_mail() -> &'static Self {
        &LABEL_ID_ALL_MAIL
    }

    #[must_use]
    pub fn archive() -> &'static Self {
        &LABEL_ID_ARCHIVE
    }

    #[must_use]
    pub fn sent() -> &'static Self {
        &LABEL_ID_SENT
    }

    #[must_use]
    pub fn drafts() -> &'static Self {
        &LABEL_ID_DRAFTS
    }

    #[must_use]
    pub fn outbox() -> &'static Self {
        &LABEL_ID_OUTBOX
    }

    #[must_use]
    pub fn starred() -> &'static Self {
        &LABEL_ID_STARRED
    }

    #[must_use]
    pub fn all_scheduled() -> &'static Self {
        &LABEL_ID_ALL_SCHEDULED
    }

    #[must_use]
    pub fn almost_all_mail() -> &'static Self {
        &LABEL_ID_ALMOST_ALL_MAIL
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
            v => Err(rusqlite::types::FromSqlError::OutOfRange(i64::from(v))),
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
