use crate::datatypes::{LabelColor, ViewMode};
use crate::models::{Label, MailSettings, MAIL_SETTINGS_ID};
use crate::AppError;
use stash::orm::Model;
use stash::stash::{AgnosticInterface, Interface};

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
///                use for finding the records.
///
pub async fn messages_counts<A>(label: &Label, interface: &A) -> Result<(u64, u64), AppError>
where
    A: Into<AgnosticInterface> + Interface,
{
    match label.view_mode(interface).await? {
        ViewMode::Conversations => Ok((label.unread_conv, label.total_conv)),
        ViewMode::Messages => Ok((label.unread_msg, label.total_msg)),
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
///                 use for finding the records.
///
/// # Panics
///
/// If there is no [`MailSettings`] in the [`Stash`]
///
pub async fn color_to_display<A>(
    value: &Label,
    interface: &A,
) -> Result<Option<LabelColor>, AppError>
where
    A: Into<AgnosticInterface> + Interface,
{
    let settings = MailSettings::load(MAIL_SETTINGS_ID.into(), interface)
        .await?
        .expect("MailSettings in Stash");

    if settings.enable_folder_color {
        if settings.inherit_parent_folder_color {
            // Get parent until there is no more parent and take its color
            let mut current = value.clone();
            while let Some(parent_id) = current.local_parent_id {
                current = Label::load(parent_id, interface)
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
