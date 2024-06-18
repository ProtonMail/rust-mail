use crate::exports::serde::{self, Deserialize, Deserializer, Serialize};
use proton_api_core::new_integer_enum;
use proton_api_core::utils::{
    bool_from_integer, bool_to_integer, opt_bool_from_integer, opt_bool_to_integer,
};

#[cfg(feature = "sql")]
use proton_api_core::exports::proton_sqlite3;
use stash::macros::Model;
use stash::orm::Model;
use stash::sql_using_serde;
use stash::stash::Stash;
use tracing::debug;
use crate::domain::ApiError;
use crate::MailSession;
use crate::requests::GetMailSettingsRequest;

pub const MAIL_SETTINGS_ID: u64 = 1;

#[derive(Clone, Debug, Eq, Deserialize, Model, PartialEq, Serialize)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
// TEMP
//#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[allow(clippy::struct_excessive_bools)]
#[TableName("settings")]
pub struct MailSettings {
    #[IdField]
    #[serde(skip)]
    pub local_id: u64,
    #[DbField]
    pub display_name: String,
    #[DbField]
    pub signature: String,
    #[DbField]
    pub theme: String,
    #[DbField]
    #[serde(
        default = "default_proto_bool_true",
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub auto_save_contacts: bool,
    #[DbField]
    #[serde(default)]
    pub composer_mode: MailSettingsComposerMode,
    #[DbField]
    #[serde(default)]
    pub message_buttons: MailSettingsMessageButtons,
    #[DbField]
    #[serde(default)]
    pub show_images: MailSettingsShowImages,
    #[DbField]
    #[serde(default)]
    pub show_moved: MailSettingsShowMoved,
    #[DbField]
    pub auto_delete_spam_and_trash_days: Option<u32>,
    #[DbField]
    #[serde(default)]
    pub almost_all_mail: MailSettingsAlmostAllMail,
    #[DbField]
    pub next_message_on_move: Option<MailSettingsNextMessageOnMove>,
    #[DbField]
    #[serde(default)]
    pub view_mode: MailSettingsViewMode,
    #[DbField]
    #[serde(default)]
    pub view_layout: MailSettingsViewLayout,
    #[DbField]
    #[serde(default)]
    pub swipe_left: MailSettingsSwipeAction,
    #[DbField]
    #[serde(default)]
    pub swipe_right: MailSettingsSwipeAction,
    #[DbField]
    #[serde(
        default = "default_proto_bool_true",
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub shortcuts: bool,
    #[DbField]
    #[serde(rename = "PMSignature", default)]
    pub pm_signature: MailSettingsPMSignature,
    #[DbField]
    #[serde(
        rename = "PMSignatureReferralLink",
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub pm_signature_referral_link: bool,
    #[DbField]
    #[serde(default)]
    pub image_proxy: u32,
    #[DbField]
    pub num_message_per_page: u32,
    #[DbField]
    #[serde(rename = "DraftMIMEType")]
    pub draft_mime_type: String,
    #[DbField]
    #[serde(rename = "ReceiveMIMEType")]
    pub receive_mime_type: String,
    #[DbField]
    #[serde(rename = "ShowMIMEType")]
    pub show_mime_type: String,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub enable_folder_color: bool,
    #[DbField]
    #[serde(
        default = "default_proto_bool_true",
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub inherit_parent_folder_color: bool,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub submission_access: bool,
    #[DbField]
    #[serde(default)]
    pub right_to_left: MailSettingsComposerDirection,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub attach_public_key: bool,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub sign: bool,
    #[DbField]
    #[serde(default, rename = "PGPScheme")]
    pub pgp_scheme: MailSettingsPGPScheme,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub prompt_pin: bool,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub sticky_labels: bool,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub confirm_link: bool,
    #[DbField]
    #[serde(default = "default_delay_seconds")]
    pub delay_send_seconds: u32,
    #[DbField]
    pub font_face: Option<String>,
    #[DbField]
    pub spam_action: Option<MailSettingsSpamAction>,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "opt_bool_from_integer",
        serialize_with = "opt_bool_to_integer"
    )]
    pub block_sender_confirmation: Option<bool>,
    #[DbField]
    pub mobile_settings: Option<MailSettingsMobileSettings>,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub hide_remote_images: bool,
    #[DbField]
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub hide_sender_images: bool,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}

impl MailSettings {
    pub async fn sync_mail_settings(mail_session: MailSession) -> Result<(), ApiError> {
        let mut settings = mail_session.session()
            .execute_request(GetMailSettingsRequest {})
            .await
            .map(|r| r.mail_settings)?;
        debug!("Storing labels into database");
        settings.save().await?;
        Ok(())
    }
}

impl Default for MailSettings {
    fn default() -> Self {
        Self {
            local_id: MAIL_SETTINGS_ID,
            display_name: String::new(),
            signature: String::new(),
            theme: String::new(),
            auto_save_contacts: true,
            composer_mode: MailSettingsComposerMode::default(),
            message_buttons: MailSettingsMessageButtons::default(),
            show_images: MailSettingsShowImages::default(),
            show_moved: MailSettingsShowMoved::default(),
            auto_delete_spam_and_trash_days: None,
            almost_all_mail: MailSettingsAlmostAllMail::default(),
            next_message_on_move: Some(MailSettingsNextMessageOnMove::DisabledExplicit),
            view_mode: MailSettingsViewMode::default(),
            view_layout: MailSettingsViewLayout::default(),
            swipe_left: MailSettingsSwipeAction::default(),
            swipe_right: MailSettingsSwipeAction::default(),
            shortcuts: true,
            pm_signature: MailSettingsPMSignature::default(),
            pm_signature_referral_link: false,
            image_proxy: 0,
            num_message_per_page: 0,
            draft_mime_type: "text/html".to_owned(),
            receive_mime_type: "text/html".to_owned(),
            show_mime_type: "text/html".to_owned(),
            enable_folder_color: false,
            inherit_parent_folder_color: true,
            submission_access: false,
            right_to_left: MailSettingsComposerDirection::LeftToRight,
            attach_public_key: false,
            sign: false,
            pgp_scheme: MailSettingsPGPScheme::Mime,
            prompt_pin: false,
            sticky_labels: false,
            confirm_link: true,
            delay_send_seconds: 0,
            font_face: None,
            spam_action: None,
            block_sender_confirmation: None,
            mobile_settings: None,
            hide_remote_images: false,
            hide_sender_images: false,
            row_id: None,
            stash: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MailSettingsAutoResponder {
    pub start_time: u64,
    pub end_time: u64,
    pub repeat: u64,
    pub days_selected: Vec<String>,
    pub subject: String,
    pub message: String,
    #[serde(default)]
    pub is_enabled: bool,
    pub zone: String,
}

new_integer_enum!(u8, MailSettingsComposerMode {
    Normal = 0,
    Maximized = 1,
});

impl Default for MailSettingsComposerMode {
    fn default() -> Self {
        Self::Normal
    }
}

new_integer_enum!(u8, MailSettingsMessageButtons {
    ReadFirst = 0,
    UnreadFirst = 1,
});

impl Default for MailSettingsMessageButtons {
    fn default() -> Self {
        Self::ReadFirst
    }
}

new_integer_enum!(u8, MailSettingsShowImages {
    DoNotAutoLoad =0,
    AutoLoadRemote =1,
    AutoLoadEmbedded =2,
    AutoLoadBoth =3,
});

impl Default for MailSettingsShowImages {
    fn default() -> Self {
        Self::AutoLoadEmbedded
    }
}

new_integer_enum!(u8, MailSettingsShowMoved {
    DoNotKeep =0,
    KeepInDrafts =1,
    KeepInSent=2,
    KeepBoth =3,
});

impl Default for MailSettingsShowMoved {
    fn default() -> Self {
        Self::DoNotKeep
    }
}

new_integer_enum!(u8, MailSettingsAlmostAllMail {
    AllMail = 0,
    AlmostAllMail=1,
});

impl Default for MailSettingsAlmostAllMail {
    fn default() -> Self {
        Self::AlmostAllMail
    }
}

new_integer_enum!(u8, MailSettingsNextMessageOnMove {
    DisabledExplicit=0,
    DisabledImplicit=1,
    EnabledExplicit=2,
});

impl Default for MailSettingsNextMessageOnMove {
    fn default() -> Self {
        Self::DisabledExplicit
    }
}

new_integer_enum!(u8, MailSettingsViewMode {
    Conversations =0,
    Messages=1,
});

impl Default for MailSettingsViewMode {
    fn default() -> Self {
        Self::Conversations
    }
}

new_integer_enum!(u8, MailSettingsViewLayout {
    Column =0,
    Row=1,
});

impl Default for MailSettingsViewLayout {
    fn default() -> Self {
        Self::Column
    }
}

new_integer_enum!(u8, MailSettingsSwipeAction {
     Trash =0,
    Spam=1,
    Star=2,
    Archive=3,
    MarkAsRead=4,
});

impl Default for MailSettingsSwipeAction {
    fn default() -> Self {
        Self::Archive
    }
}

new_integer_enum!(u8, MailSettingsComposerDirection {
    LeftToRight = 0,
    RightToLeft = 1,
});

impl Default for MailSettingsComposerDirection {
    fn default() -> Self {
        Self::LeftToRight
    }
}

new_integer_enum!(u8, MailSettingsPGPScheme {
    Inline = 8,
    Mime = 16,
});

impl Default for MailSettingsPGPScheme {
    fn default() -> Self {
        Self::Mime
    }
}

new_integer_enum!(u8, MailSettingsSpamAction {
    DoNothing =0,
    UnsubscribeWithOneClick = 1,
});

new_integer_enum!(u8, MailSettingsPMSignature {
    Disabled = 0,
    Enabled=1,
    EnabledLocked=2,
});

impl Default for MailSettingsPMSignature {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct MailSettingsMobileSetting {
    pub is_custom: bool,
    #[serde(default)]
    pub actions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct MailSettingsMobileSettings {
    pub message_toolbar: MailSettingsMobileSetting,
    pub conversation_toolbar: MailSettingsMobileSetting,
    pub list_toolbar: MailSettingsMobileSetting,
}

sql_using_serde!(MailSettingsMobileSettings);

#[inline]
fn default_proto_bool_true() -> bool {
    true
}

#[inline]
fn default_delay_seconds() -> u32 {
    10
}

pub fn deserialize_pm_signature<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let v = u32::deserialize(deserializer)?;
    Ok(v > 0)
}
