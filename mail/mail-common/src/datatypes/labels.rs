use crate::AppError;
use crate::datatypes::{LabelColor, ViewMode};
use crate::models::{ConversationCounters, MailLabel, MailSettings, MessageCounters};
use proton_core_common::models::{Label, ModelExtension};
use stash::orm::Model;
use stash::stash::Tether;

pub mod custom_folder;
pub mod custom_labels;
pub mod hierarchy;
pub mod system_labels;

/// Get the the counts (first unread, second total) depending on the [`ViewMode`].
///
/// # Parameters
///
/// * `label`    - The [`Label`]
/// * `inteface` - The database interface, i.e. [`Stash`] or [`Tether`], to
///   use for finding the records.
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
/// # Parameters
///
/// * `value`     - The [`Label`]
/// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
///   use for finding the records.
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
