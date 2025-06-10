use std::collections::BTreeSet;
use std::sync::Arc;

use crate::AppError;
use crate::datatypes::{
    AlmostAllMail, ComposerDirection, ComposerMode, MailSettingsId, MessageButtons, MimeType,
    MobileSettings, NextMessageOnMove, PgpScheme, PmSignature, ShowImages, ShowMoved, SpamAction,
    SwipeAction, ViewLayout, ViewMode,
};
use proton_core_common::datatypes::InitializationKey;
use proton_core_common::models::{
    InitializationError, InitializationWatcher, InitializedComponent,
};
use proton_crypto_inbox::keys::CryptoMailSettings;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::response_data::MailSettings as ApiMailSettings;
use smart_default::SmartDefault;
use sqlite_watcher::watcher::TableObserver;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Bond, Stash, StashError, Tether, WatcherHandle};
use tracing::debug;

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
    #[IdField]
    pub local_id: MailSettingsId,

    #[DbField]
    pub almost_all_mail: AlmostAllMail,

    #[DbField]
    pub attach_public_key: bool,

    #[DbField]
    pub auto_delete_spam_and_trash_days: Option<u32>,

    #[DbField]
    #[default = true]
    pub auto_save_contacts: bool,

    #[DbField]
    pub block_sender_confirmation: Option<bool>,

    #[DbField]
    pub composer_mode: ComposerMode,

    #[DbField]
    #[default = true]
    pub confirm_link: bool,

    #[DbField]
    #[default = 10]
    pub delay_send_seconds: u32,

    #[DbField]
    pub display_name: String,

    #[DbField]
    pub draft_mime_type: MimeType,

    #[DbField]
    pub enable_folder_color: bool,

    #[DbField]
    pub font_face: Option<String>,

    #[DbField]
    pub hide_remote_images: bool,

    #[DbField]
    pub hide_embedded_images: bool,

    #[DbField]
    pub hide_sender_images: bool,

    #[DbField]
    pub image_proxy: u32,

    #[DbField]
    #[default = true]
    pub inherit_parent_folder_color: bool,

    #[DbField]
    pub message_buttons: MessageButtons,

    #[DbField]
    pub mobile_settings: Option<MobileSettings>,

    #[DbField]
    pub next_message_on_move: Option<NextMessageOnMove>,

    #[DbField]
    pub num_message_per_page: u32,

    #[DbField]
    pub pgp_scheme: PgpScheme,

    #[DbField]
    pub pm_signature: PmSignature,

    #[DbField]
    #[default = true]
    pub pm_signature_referral_link: bool,

    #[DbField]
    pub prompt_pin: bool,

    #[DbField]
    pub receive_mime_type: MimeType,

    #[DbField]
    pub right_to_left: ComposerDirection,

    #[DbField]
    #[default = true]
    pub shortcuts: bool,

    #[DbField]
    pub show_images: ShowImages,

    #[DbField]
    pub show_mime_type: MimeType,

    #[DbField]
    pub show_moved: ShowMoved,

    #[DbField]
    pub sign: bool,

    #[DbField]
    pub signature: String,

    #[DbField]
    pub spam_action: Option<SpamAction>,

    #[DbField]
    pub sticky_labels: bool,

    #[DbField]
    pub submission_access: bool,

    #[DbField]
    pub swipe_left: SwipeAction,

    #[DbField]
    pub swipe_right: SwipeAction,

    #[DbField]
    pub theme: String,

    #[DbField]
    pub view_layout: ViewLayout,

    #[DbField]
    pub view_mode: ViewMode,

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl MailSettings {
    /// Key used to distinguish between components in the initialization.
    /// It is a string, not an enum for making it open for additional changes from different BU.
    ///
    pub const INIT_KEY: InitializationKey = InitializationKey::new("mail_settings");

    /// It initializes mail settings by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize<PM: ProtonMail>(
        watcher: Arc<InitializationWatcher>,
        api: &PM,
        stash: &Stash,
    ) -> Result<(), InitializationError<AppError>> {
        InitializedComponent::initialize::<AppError, SyncedMailSettings>(
            watcher,
            Self::INIT_KEY,
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
    pub async fn store(mut self, tx: &Bond<'_>) -> Result<(), StashError> {
        self.settings.save(tx).await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/models/settings.rs"]
mod settings;
