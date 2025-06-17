use crate::AppError;
use crate::datatypes::{LabelColor, SystemLabelId, ViewMode};
use crate::models::{ConversationCounters, MailLabel, MailSettings, MessageCounters};
use proton_core_api::services::proton::LabelId;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{Label, ModelExtension, ModelIdExtension};
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

/// Get the the counts (first unread, second total) depending on the [`ViewMode`].
///
/// # Panics
/// If label provided does not have Local ID
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

/// Get the color a [`Label`] should be displayed with.
///
/// The color depends on [`MailSettings`] `enable_folder_color` and `inherit_parent_folder_color`
///
///
/// # Panics
///
/// If there is no [`MailSettings`] in the [`Stash`]
///
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
pub enum LabelScrollOrder {
    Ascending = 0,
    #[default]
    Descending = 1,
}

impl LabelScrollOrder {
    pub fn for_label_id(id: &LabelId) -> Self {
        if *id == LabelId::all_scheduled() {
            Self::Ascending
        } else {
            Self::Descending
        }
    }

    pub async fn for_local_label_id(id: LocalLabelId, tether: &Tether) -> Result<Self, StashError> {
        if let Some(remote_id) = Label::local_id_counterpart(id, tether).await? {
            Ok(Self::for_label_id(&remote_id))
        } else {
            Ok(Self::default())
        }
    }
}

impl ToSql for LabelScrollOrder {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

impl FromSql for LabelScrollOrder {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match i64::column_result(value)? {
            0 => Ok(Self::Ascending),
            1 => Ok(Self::Descending),
            v => Err(FromSqlError::OutOfRange(v)),
        }
    }
}
