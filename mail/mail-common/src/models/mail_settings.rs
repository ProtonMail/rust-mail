use std::collections::BTreeSet;

use crate::AppError;
use crate::datatypes::{
    AlmostAllMail, ComposerDirection, ComposerMode, InitializedComponentKey, MailSettingsId,
    MessageButtons, MimeType, MobileSettings, NextMessageOnMove, PgpScheme, PmSignature,
    ShowImages, ShowMoved, SpamAction, SwipeAction, ViewLayout, ViewMode,
};
use proton_api_mail::services::proton::ProtonMail;
use proton_api_mail::services::proton::response_data::MailSettings as ApiMailSettings;
use proton_crypto_inbox::keys::CryptoMailSettings;
use smart_default::SmartDefault;
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use tracing::debug;

use super::{InitializationError, InitializedComponent};

/// Mail related use settings.
///
/// # Remarks
///
/// To correctly use this class please use [`MailSettings::get()`] or
/// [`MailSettings::get_or_default()`] to load the mail settings.
#[derive(Clone, Debug, Eq, Model, PartialEq, SmartDefault)]
#[allow(clippy::struct_excessive_bools)]
#[TableName("mail_settings")]
pub struct MailSettings {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField]
    pub local_id: MailSettingsId,

    /// TODO: Document this field.
    #[DbField]
    pub almost_all_mail: AlmostAllMail,

    /// TODO: Document this field.
    #[DbField]
    pub attach_public_key: bool,

    /// TODO: Document this field.
    #[DbField]
    pub auto_delete_spam_and_trash_days: Option<u32>,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub auto_save_contacts: bool,

    /// TODO: Document this field.
    #[DbField]
    pub block_sender_confirmation: Option<bool>,

    /// TODO: Document this field.
    #[DbField]
    pub composer_mode: ComposerMode,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub confirm_link: bool,

    /// TODO: Document this field.
    #[DbField]
    #[default = 10]
    pub delay_send_seconds: u32,

    /// TODO: Document this field.
    #[DbField]
    pub display_name: String,

    /// TODO: Document this field.
    #[DbField]
    pub draft_mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub enable_folder_color: bool,

    /// TODO: Document this field.
    #[DbField]
    pub font_face: Option<String>,

    /// This enables or disables remote content in the HTML.
    #[DbField]
    pub hide_remote_images: bool,

    /// This enables or disables embedded content (`Disposition::Inline`) in the HTML.
    #[DbField]
    pub hide_embedded_images: bool,

    /// TODO: Document this field.
    #[DbField]
    pub hide_sender_images: bool,

    /// TODO: Document this field.
    #[DbField]
    pub image_proxy: u32,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub inherit_parent_folder_color: bool,

    /// TODO: Document this field.
    #[DbField]
    pub message_buttons: MessageButtons,

    /// TODO: Document this field.
    #[DbField]
    pub mobile_settings: Option<MobileSettings>,

    /// TODO: Document this field.
    #[DbField]
    pub next_message_on_move: Option<NextMessageOnMove>,

    /// TODO: Document this field.
    #[DbField]
    pub num_message_per_page: u32,

    /// TODO: Document this field.
    #[DbField]
    pub pgp_scheme: PgpScheme,

    /// TODO: Document this field.
    #[DbField]
    pub pm_signature: PmSignature,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub pm_signature_referral_link: bool,

    /// TODO: Document this field.
    #[DbField]
    pub prompt_pin: bool,

    /// TODO: Document this field.
    #[DbField]
    pub receive_mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub right_to_left: ComposerDirection,

    /// TODO: Document this field.
    #[DbField]
    #[default = true]
    pub shortcuts: bool,

    /// TODO: Document this field.
    #[DbField]
    pub show_images: ShowImages,

    /// TODO: Document this field.
    #[DbField]
    pub show_mime_type: MimeType,

    /// TODO: Document this field.
    #[DbField]
    pub show_moved: ShowMoved,

    /// TODO: Document this field.
    #[DbField]
    pub sign: bool,

    /// TODO: Document this field.
    #[DbField]
    pub signature: String,

    /// TODO: Document this field.
    #[DbField]
    pub spam_action: Option<SpamAction>,

    /// TODO: Document this field.
    #[DbField]
    pub sticky_labels: bool,

    /// TODO: Document this field.
    #[DbField]
    pub submission_access: bool,

    /// TODO: Document this field.
    #[DbField]
    pub swipe_left: SwipeAction,

    /// TODO: Document this field.
    #[DbField]
    pub swipe_right: SwipeAction,

    /// TODO: Document this field.
    #[DbField]
    pub theme: String,

    /// TODO: Document this field.
    #[DbField]
    pub view_layout: ViewLayout,

    /// TODO: Document this field.
    #[DbField]
    pub view_mode: ViewMode,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl MailSettings {
    /// It initializes mail settings by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize<PM: ProtonMail>(
        api: &PM,
        stash: &Stash,
    ) -> Result<(), InitializationError<AppError>> {
        InitializedComponent::initialize::<AppError, SyncedMailSettings>(
            InitializedComponentKey::MailSettings,
            &[],
            stash.connection(),
            async move || Self::sync_mail_settings(api).await,
            async |tx, res| {
                res.store(tx).await?;
                Ok(())
            },
        )
        .await
    }

    /// TODO: Document this function.
    ///
    /// # Parameters
    ///
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn sync_mail_settings<PM: ProtonMail>(
        api: &PM,
    ) -> Result<SyncedMailSettings, AppError> {
        debug!("Storing settings into database");
        let settings = MailSettings::from(api.get_mail_settings().await?.mail_settings);

        Ok(SyncedMailSettings { settings })
    }

    /// Get the mail settings from database
    pub async fn get(tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::load(MailSettingsId, tether).await
    }

    /// Save or update a mail setting.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is updated correctly in the database.
    ///
    /// This method ensures that there is only one mail setting in the table.
    /// Otherwise, it overwrites old record.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        // // Make sure there will be only one row.
        if let Some(existing) = Self::get(bond).await? {
            self.row_id = existing.row_id;
            self.local_id = MailSettingsId;
        }

        <Self as Model>::save(self, bond).await
    }

    /// Get the mail settings from database, fallback on default
    pub async fn get_or_default(tether: &Tether) -> Self {
        Self::get(tether)
            .await
            .unwrap_or_default()
            .unwrap_or_default()
    }

    /// Retrieves the portion of the mail settings that is relevant for email encryption.
    pub fn crypto_mail_settings(&self) -> CryptoMailSettings {
        CryptoMailSettings {
            pgp_scheme: self.pgp_scheme.into(),
            mime_type: self.draft_mime_type.into(),
            sign: self.sign,
        }
    }

    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(MailSettingsWatcher { sender }))
    }
}

pub struct MailSettingsWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for MailSettingsWatcher {
    fn tables(&self) -> Vec<String> {
        vec![MailSettings::table_name().to_string()]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for MailSettingsWatcher: {:?}",
                    e
                )
            })
            .ok();
    }
}

impl From<ApiMailSettings> for MailSettings {
    fn from(value: ApiMailSettings) -> Self {
        Self {
            local_id: MailSettingsId,
            almost_all_mail: value.almost_all_mail.into(),
            attach_public_key: value.attach_public_key,
            auto_delete_spam_and_trash_days: value.auto_delete_spam_and_trash_days,
            auto_save_contacts: value.auto_save_contacts,
            block_sender_confirmation: value.block_sender_confirmation,
            composer_mode: value.composer_mode.into(),
            confirm_link: value.confirm_link,
            delay_send_seconds: value.delay_send_seconds,
            display_name: value.display_name,
            draft_mime_type: value.draft_mime_type.into(),
            enable_folder_color: value.enable_folder_color,
            font_face: value.font_face,
            hide_remote_images: value.hide_remote_images,
            hide_embedded_images: value.hide_embedded_images,
            hide_sender_images: value.hide_sender_images,
            image_proxy: value.image_proxy,
            inherit_parent_folder_color: value.inherit_parent_folder_color,
            message_buttons: value.message_buttons.into(),
            mobile_settings: value.mobile_settings.map(Into::into),
            next_message_on_move: value.next_message_on_move.map(Into::into),
            num_message_per_page: value.num_message_per_page,
            pgp_scheme: value.pgp_scheme.into(),
            pm_signature: value.pm_signature.into(),
            pm_signature_referral_link: value.pm_signature_referral_link,
            prompt_pin: value.prompt_pin,
            receive_mime_type: value.receive_mime_type.into(),
            right_to_left: value.right_to_left.into(),
            shortcuts: value.shortcuts,
            show_images: value.show_images.into(),
            show_mime_type: value.show_mime_type.into(),
            show_moved: value.show_moved.into(),
            sign: value.sign,
            signature: value.signature,
            spam_action: value.spam_action.map(Into::into),
            sticky_labels: value.sticky_labels,
            submission_access: value.submission_access,
            swipe_left: value.swipe_left.into(),
            swipe_right: value.swipe_right.into(),
            theme: value.theme,
            view_layout: value.view_layout.into(),
            view_mode: value.view_mode.into(),
            row_id: None,
        }
    }
}

/// This is a manual implementation of `MailSettings::sync_mail_settings` async closure.
///
/// We keep it as it is until Rust allows us to use `impl Trait` in generics etc.
#[must_use]
#[derive(Debug)]
pub struct SyncedMailSettings {
    settings: MailSettings,
}

impl SyncedMailSettings {
    /// Consume this manual closure by storing data in the Database.
    ///
    #[tracing::instrument(skip(tx))]
    pub async fn store(mut self, tx: &Bond<'_>) -> Result<(), AppError> {
        self.settings.save(tx).await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/models/settings.rs"]
mod settings;
