use crate::domain::UserId;
use crate::requests::FIDOKey;
use crate::utils::{bool_from_integer, bool_to_integer};
use serde::{Deserialize, Serialize};
use serde_aux::field_attributes::deserialize_default_from_null;
use stash::macros::Model;
use stash::stash::Stash;
use stash::utils::sql_using_serde;

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

sql_using_serde!(Email);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Phone {
    #[serde(deserialize_with = "deserialize_default_from_null")]
    pub value: String,
    pub status: u8,
    pub notify: u8,
    pub reset: u8,
}

sql_using_serde!(Phone);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct TwoFA {
    pub enabled: TFAStatus,
    pub allowed: TFAStatus,
    pub expiration_time: Option<u64>,
    #[serde(default)]
    pub registered_keys: Vec<FIDOKey>,
}

sql_using_serde!(TwoFA);

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

sql_using_serde!(SettingsFlags);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Referral {
    pub link: String,
    pub eligible: bool,
}

sql_using_serde!(Referral);

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

sql_using_serde!(HighSecurity);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Password {
    pub mode: u32,
    pub expiration_time: Option<u64>,
}

sql_using_serde!(Password);

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Deserialize, Eq, Model, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
#[TableName("user_settings")]
pub struct UserSettings {
    #[IdField]
    pub id: Option<UserId>,
    #[DbField]
    pub email: Email,
    #[DbField]
    pub password: Password,
    #[DbField]
    pub phone: Phone,
    #[DbField]
    pub two_factor_auth: TwoFA,
    #[DbField]
    pub news: u32,
    #[DbField]
    pub locale: String,
    #[DbField]
    pub log_auth: LogAuth,
    #[DbField]
    pub invoice_text: String,
    #[DbField]
    pub density: Density,
    #[DbField]
    pub week_start: WeekStart,
    #[DbField]
    pub date_format: DateFormat,
    #[DbField]
    pub time_format: TimeFormat,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub welcome: bool,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub early_access: bool,
    #[DbField]
    pub flags: SettingsFlags,
    #[DbField]
    pub referral: Option<Referral>,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    #[DbField]
    pub device_recovery: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    #[DbField]
    pub telemetry: bool,
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    #[DbField]
    pub crash_reports: bool,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub hide_side_panel: bool,
    #[DbField]
    pub high_security: HighSecurity,
    #[DbField]
    #[serde(
        deserialize_with = "bool_from_integer",
        serialize_with = "bool_to_integer"
    )]
    pub session_account_recovery: bool,
    #[RowIdField]
    #[serde(skip)]
    pub row_id: Option<u64>,
    #[StashField]
    #[serde(skip)]
    pub stash: Option<Stash>,
}
