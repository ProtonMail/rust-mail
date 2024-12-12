use crate::datatypes::{
    AlmostAllMail, ComposerDirection, ComposerMode, MessageButtons, MimeType, MobileSettings,
    NextMessageOnMove, PgpScheme, PmSignature, ShowImages, ShowMoved, SpamAction, SwipeAction,
    ViewLayout, ViewMode,
};
use crate::AppError;
use proton_api_mail::services::proton::response_data::MailSettings as ApiMailSettings;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::LocalId;
use proton_crypto_inbox::keys::CryptoMailSettings;
use smart_default::SmartDefault;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{Stash, StashError, Tether};
use tracing::debug;

pub const MAIL_SETTINGS_ID: u64 = 1;

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq, SmartDefault)]
#[allow(clippy::struct_excessive_bools)]
#[TableName("mail_settings")]
pub struct MailSettings {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

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
        stash: &Stash,
    ) -> Result<(), AppError> {
        let mut settings = MailSettings::from(api.get_settings().await.map(|r| r.mail_settings)?);
        debug!("Storing labels into database");

        let mut tether = stash.connection();
        let tx = tether.transaction().await?;
        settings.save(&tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Get the mail settings from database
    pub async fn get(tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::load(MAIL_SETTINGS_ID.into(), tether).await
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
}

impl From<ApiMailSettings> for MailSettings {
    fn from(value: ApiMailSettings) -> Self {
        Self {
            local_id: None,
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

#[cfg(test)]
#[path = "../tests/models/settings.rs"]
mod settings;
