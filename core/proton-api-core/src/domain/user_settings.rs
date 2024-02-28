use crate::domain::ProtonBoolean;
use crate::requests::FIDOKey;
use serde::{Deserialize, Serialize};

new_integer_enum!(u8,TFAStatus {
    None = 0,
    Totp = 1,
    FIDO2 = 2,
    TotpOrFIDO2 = 3,
});

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettingsEmail {
    pub value: String,
    pub status: u8,
    pub notify: u8,
    pub reset: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettingsPhone {
    pub value: String,
    pub status: u8,
    pub notify: u8,
    pub reset: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettings2FA {
    pub enabled: TFAStatus,
    pub allowed: TFAStatus,
    pub expiration_time: u64,
    #[serde(default)]
    pub registered_keys: Vec<FIDOKey>,
}

new_integer_enum!(u8, UserLogAuth {
    Disabled =0,
    Basic=1,
    Advanced=2,
});

new_integer_enum!(u8, UserSettingsDensity {
    Comfortable = 0,
    Compact =1,
});

new_integer_enum!(u8, UserSettingsWeekStart {
    Default =0,
    Monday =1,
    Saturday =6,
    Sunday=7,
});

new_integer_enum!(u8, UserSettingsDateFormat {
    Default =0,
    DDMMYYYY=1,
    MMDDYYYY=2,
    YYYYMMDD=3,
});

new_integer_enum!(u8, UserSettingsTimeFormat {
    Default=0,
    H24=1,
    H12=2,
});

new_integer_enum!(u8, UserSettingsEarlyAccess {
    Regular=0,
    Beta=1,
});

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettingsFlags {
    pub welcomed: ProtonBoolean,
    pub in_app_promos_hidden: ProtonBoolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettingsReferral {
    pub link: String,
    pub eligible: ProtonBoolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettingsHighSecurity {
    pub eligible: ProtonBoolean,
    pub value: ProtonBoolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettingsPassword {
    pub mode: u32,
    pub expiration_time: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettings {
    pub email: UserSettingsEmail,
    pub password: UserSettingsPassword,
    pub phone: UserSettingsPhone,
    #[serde(rename = "2FA")]
    pub two_factor_auth: UserSettings2FA,
    pub news: u8,
    pub locale: String,
    pub log_auth: UserLogAuth,
    pub invoice_text: String,
    pub density: UserSettingsDensity,
    pub week_start: UserSettingsWeekStart,
    pub date_format: UserSettingsDateFormat,
    pub time_format: UserSettingsTimeFormat,
    pub welcome: ProtonBoolean,
    pub early_access: ProtonBoolean,
    pub flags: UserSettingsFlags,
    pub referral: Option<UserSettingsReferral>,
    pub device_recovery: ProtonBoolean,
    pub telemetry: ProtonBoolean,
    pub crash_reports: ProtonBoolean,
    pub hide_side_panel: ProtonBoolean,
    pub high_security: UserSettingsHighSecurity,
    pub session_account_recovery: ProtonBoolean,
}
