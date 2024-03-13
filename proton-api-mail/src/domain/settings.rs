use crate::exports::serde::{self, Deserialize, Serialize};
use proton_api_core::domain::ProtonBoolean;
use proton_api_core::new_integer_enum;
use proton_api_core::exports::proton_sqlite3;

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MailSettings {
    pub display_name: String,
    pub signature: String,
    pub theme: String,
    #[serde(default = "default_proto_bool_true")]
    pub auto_save_contacts: ProtonBoolean,
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
    #[serde(default = "default_proto_bool_true")]
    pub shortcuts: ProtonBoolean,
    #[serde(rename = "PMSignature", default)]
    pub pm_signature: ProtonBoolean,
    #[serde(rename = "PMSignatureReferralLink", default)]
    pub pm_signature_referral_link: ProtonBoolean,
    #[serde(default)]
    pub image_proxy: u32,
    pub num_message_per_page: u32,
    #[serde(rename = "DraftMIMEType")]
    pub draft_mime_type: String,
    #[serde(rename = "ReceiveMIMEType")]
    pub receive_mime_type: String,
    #[serde(rename = "ShowMIMEType")]
    pub show_mime_type: String,
    #[serde(default)]
    pub enable_folder_color: ProtonBoolean,
    #[serde(default = "default_proto_bool_true")]
    pub inherit_parent_folder_color: ProtonBoolean,
    #[serde(default)]
    pub submission_access: ProtonBoolean,
    #[serde(default)]
    pub right_to_left: MailSettingsComposerDirection,
    #[serde(default)]
    pub attach_public_key: ProtonBoolean,
    #[serde(default)]
    pub sign: ProtonBoolean,
    #[serde(default, rename = "PGPScheme")]
    pub pgp_scheme: MailSettingsPGPScheme,
    #[serde(default)]
    pub prompt_pin: ProtonBoolean,
    #[serde(default)]
    pub sticky_labels: ProtonBoolean,
    #[serde(default)]
    pub confirm_link: ProtonBoolean,
    #[serde(default = "default_delay_seconds")]
    pub delay_send_seconds: u32,
    pub font_face: Option<String>,
    pub spam_action: Option<MailSettingsSpamAction>,
    pub block_sender_confirmation: Option<ProtonBoolean>,
    pub mobile_settings: Option<MailSettingsMobileSettings>,
    #[serde(default)]
    pub hide_remote_images: ProtonBoolean,
    #[serde(default)]
    pub hide_sender_images: ProtonBoolean,
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

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MailSettingsMobileSetting {
    pub is_custom: bool,
    #[serde(default)]
    pub actions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
#[serde(crate = "self::serde", rename_all = "PascalCase")]
pub struct MailSettingsMobileSettings {
    pub message_toolbar: MailSettingsMobileSetting,
    pub conversation_toolbar: MailSettingsMobileSetting,
    pub list_toolbar: MailSettingsMobileSetting,
}

#[inline]
fn default_proto_bool_true() -> ProtonBoolean {
    ProtonBoolean::True
}

#[inline]
fn default_delay_seconds() -> u32 {
    10
}
