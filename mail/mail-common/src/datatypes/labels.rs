use crate::AppError;
use crate::datatypes::{LabelColor, SystemLabelId, ViewMode};
use crate::models::{ConversationCounters, MailLabel, MailSettings, MessageCounters};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::{LocalLabelId, SystemLabel};
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::prelude::MessageMetadataSortMode;
use proton_sqlite3::rusqlite::types::{
    FromSql, FromSqlError, FromSqlResult, ToSqlOutput, Value, ValueRef,
};
use proton_sqlite3::rusqlite::{Error as SqliteError, ToSql};
use stash::orm::Model;
use stash::stash::{StashError, Tether};

pub mod custom_folder;
pub mod custom_labels;
pub mod hierarchy;
pub mod system_labels;

pub async fn messages_counts(label: &Label, tether: &Tether) -> Result<(u64, u64), AppError> {
    match label.view_mode(tether).await? {
        ViewMode::Conversations => {
            let counters = ConversationCounters::find_by_id(label.id(), tether).await?;
            let (unread, total) = counters.map(|c| c.counters()).unwrap_or_default();
            Ok((unread, total))
        }
        ViewMode::Messages => {
            let counters = MessageCounters::find_by_id(label.id(), tether).await?;
            let (unread, total) = counters.map(|c| c.counters()).unwrap_or_default();
            Ok((unread, total))
        }
    }
}

pub async fn color_to_display(
    value: &Label,
    tether: &Tether,
) -> Result<Option<LabelColor>, AppError> {
    let settings = MailSettings::get_or_default(tether).await;

    if settings.enable_folder_color {
        if settings.inherit_parent_folder_color {
            // Get parent until there is no more parent and take its color
            let mut current = value.clone();
            while let Some(parent_id) = current.local_parent_id {
                current = Label::load(parent_id, tether)
                    .await?
                    .ok_or(AppError::LabelNotFound(parent_id))?;
            }
            Ok(Some(current.color))
        } else {
            Ok(Some(value.color.clone()))
        }
    } else {
        Ok(None)
    }
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum ScrollOrderDir {
    Asc = 0,
    #[default]
    Desc = 1,
}

impl ScrollOrderDir {
    pub fn for_label(id: &LabelId) -> Self {
        if *id == LabelId::all_scheduled() || *id == LabelId::snoozed() {
            Self::Asc
        } else {
            Self::Desc
        }
    }

    pub async fn for_local_label(id: LocalLabelId, tether: &Tether) -> Result<Self, StashError> {
        if let Some(remote_id) = Label::local_id_counterpart(id, tether).await? {
            Ok(Self::for_label(&remote_id))
        } else {
            Ok(Self::default())
        }
    }

    pub fn as_api_desc(&self) -> Option<bool> {
        Some(*self == Self::Desc)
    }

    pub fn reverse(&self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
}

impl ToSql for ScrollOrderDir {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for ScrollOrderDir {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match i64::column_result(value)? {
            0 => Ok(Self::Asc),
            1 => Ok(Self::Desc),
            v => Err(FromSqlError::OutOfRange(v)),
        }
    }
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum ScrollOrderField {
    #[default]
    Time = 0,
    SnoozeTime = 1,
}

impl ScrollOrderField {
    pub fn for_label(id: &LabelId) -> Self {
        match SystemLabel::from_rid(id) {
            Some(label) if label.is_snooze_location() => Self::SnoozeTime,
            _ => Self::Time,
        }
    }

    pub async fn for_local_label(id: LocalLabelId, tether: &Tether) -> Result<Self, StashError> {
        if let Some(remote_id) = Label::local_id_counterpart(id, tether).await? {
            Ok(Self::for_label(&remote_id))
        } else {
            Ok(Self::default())
        }
    }

    pub fn as_api_sort(&self) -> Option<MessageMetadataSortMode> {
        match self {
            Self::Time => Some(MessageMetadataSortMode::Time),
            Self::SnoozeTime => Some(MessageMetadataSortMode::SnoozeTime),
        }
    }
}

impl ToSql for ScrollOrderField {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for ScrollOrderField {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match i64::column_result(value)? {
            0 => Ok(Self::Time),
            1 => Ok(Self::SnoozeTime),
            v => Err(FromSqlError::OutOfRange(v)),
        }
    }
}
