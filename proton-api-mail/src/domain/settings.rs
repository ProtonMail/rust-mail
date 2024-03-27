use crate::exports::serde::{self, Deserialize, Deserializer, Serialize};
use proton_api_core::exports::proton_sqlite3;
use proton_api_core::new_integer_enum;
use proton_api_core::utils::{
    bool_from_integer, bool_to_integer, opt_bool_from_integer, opt_bool_to_integer,
};

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct MailSettings {
    pub display_name: String,
    pub signature: String,
    pub theme: String,
    #[serde(
        default = "default_proto_bool_true",
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub auto_save_contacts: bool,
    #[serde(default)]
    pub composer_mode: MailSettingsComposerMode,
    #[serde(default)]
    pub message_buttons: MailSettingsMessageButtons,
    #[serde(default)]
    pub show_images: MailSettingsShowImages,
    #[serde(default)]
    pub show_moved: MailSettingsShowMoved,
    pub auto_delete_spam_and_trash_days: Option<u32>,
    #[serde(default)]
    pub almost_all_mail: MailSettingsAlmostAllMail,
    pub next_message_on_move: Option<MailSettingsNextMessageOnMove>,
    #[serde(default)]
    pub view_mode: MailSettingsViewMode,
    #[serde(default)]
    pub view_layout: MailSettingsViewLayout,
    #[serde(default)]
    pub swipe_left: MailSettingsSwipeAction,
    #[serde(default)]
    pub swipe_right: MailSettingsSwipeAction,
    #[serde(
        default = "default_proto_bool_true",
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub shortcuts: bool,
    #[serde(rename = "PMSignature", default)]
    pub pm_signature: MailSettingsPMSignature,
    #[serde(rename = "PMSignatureReferralLink", default)]
    pub pm_signature_referral_link: bool,
    #[serde(default)]
    pub image_proxy: u32,
    pub num_message_per_page: u32,
    #[serde(rename = "DraftMIMEType")]
    pub draft_mime_type: String,
    #[serde(rename = "ReceiveMIMEType")]
    pub receive_mime_type: String,
    #[serde(rename = "ShowMIMEType")]
    pub show_mime_type: String,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub enable_folder_color: bool,
    #[serde(
        default = "default_proto_bool_true",
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub inherit_parent_folder_color: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub submission_access: bool,
    #[serde(default)]
    pub right_to_left: MailSettingsComposerDirection,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub attach_public_key: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub sign: bool,
    #[serde(default, rename = "PGPScheme")]
    pub pgp_scheme: MailSettingsPGPScheme,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub prompt_pin: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub sticky_labels: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub confirm_link: bool,
    #[serde(default = "default_delay_seconds")]
    pub delay_send_seconds: u32,
    pub font_face: Option<String>,
    pub spam_action: Option<MailSettingsSpamAction>,
    #[serde(
        default,
        deserialize_with = "opt_bool_from_integer",
        serialize_with = "opt_bool_to_integer"
    )]
    pub block_sender_confirmation: Option<bool>,
    pub mobile_settings: Option<MailSettingsMobileSettings>,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub hide_remote_images: bool,
    #[serde(
        default,
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub hide_sender_images: bool,
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
        Self::AllMail
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
