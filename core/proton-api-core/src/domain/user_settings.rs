use crate::requests::FIDOKey;
use crate::utils::{bool_from_integer, bool_to_integer};
use serde::{Deserialize, Serialize};
use serde_aux::field_attributes::deserialize_default_from_null;

new_integer_enum!(u8,TFAStatus {
    None = 0,
    Totp = 1,
    FIDO2 = 2,
    TotpOrFIDO2 = 3,
});

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Email {
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub value: String,
    pub status: u8,
    pub notify: u8,
    pub reset: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Phone {
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub value: String,
    pub status: u8,
    pub notify: u8,
    pub reset: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct TwoFA {
    pub enabled: TFAStatus,
    pub allowed: TFAStatus,
    pub expiration_time: Option<u64>,
    #[serde(default)]
    pub registered_keys: Vec<FIDOKey>,
}

new_integer_enum!(u8, LogAuth {
    Disabled =0,
    Basic=1,
    Advanced=2,
});

new_integer_enum!(u8, Density {
    Comfortable = 0,
    Compact =1,
});

new_integer_enum!(u8, WeekStart {
    Default =0,
    Monday =1,
    Saturday =6,
    Sunday=7,
});

new_integer_enum!(u8, DateFormat {
    Default =0,
    DDMMYYYY=1,
    MMDDYYYY=2,
    YYYYMMDD=3,
});

new_integer_enum!(u8, TimeFormat {
    Default=0,
    H24=1,
    H12=2,
});

new_integer_enum!(u8, EarlyAccess {
    Regular=0,
    Beta=1,
});

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SettingsFlags {
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub welcomed: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub in_app_promos_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Referral {
    pub link: String,
    pub eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct HighSecurity {
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub eligible: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub value: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Password {
    pub mode: u32,
    pub expiration_time: Option<u64>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UserSettings {
    pub email: Email,
    pub password: Password,
    pub phone: Phone,
    #[serde(rename = "2FA")]
    pub two_factor_auth: TwoFA,
    pub news: u32,
    pub locale: String,
    pub log_auth: LogAuth,
    pub invoice_text: String,
    pub density: Density,
    pub week_start: WeekStart,
    pub date_format: DateFormat,
    pub time_format: TimeFormat,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub welcome: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub early_access: bool,
    pub flags: SettingsFlags,
    pub referral: Option<Referral>,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub device_recovery: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub telemetry: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub crash_reports: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub hide_side_panel: bool,
    pub high_security: HighSecurity,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub session_account_recovery: bool,
}
