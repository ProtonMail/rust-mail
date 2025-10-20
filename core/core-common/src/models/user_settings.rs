use crate::datatypes::{
    DateFormat, Density, Email, HighSecurity, LogAuth, NotificationSettings, Password, Phone,
    Referral, SettingsFlags, TimeFormat, TwoFa, WeekStart,
};
use proton_core_api::services::proton::UserId;
use proton_core_api::services::proton::UserSettings as ApiUserSettings;
use stash::macros::Model;

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("user_settings")]
#[allow(clippy::struct_excessive_bools)]
pub struct UserSettings {
    #[IdField(optional)]
    pub remote_id: Option<UserId>,

    #[DbField]
    pub crash_reports: bool,

    #[DbField]
    pub date_format: DateFormat,

    #[DbField]
    pub density: Density,

    #[DbField]
    pub device_recovery: bool,

    #[DbField]
    pub early_access: bool,

    #[DbField]
    pub email: Email,

    #[DbField]
    pub flags: SettingsFlags,

    #[DbField]
    pub hide_side_panel: bool,

    #[DbField]
    pub high_security: HighSecurity,

    #[DbField]
    pub invoice_text: String,

    #[DbField]
    pub locale: String,

    #[DbField]
    pub log_auth: LogAuth,

    #[DbField]
    pub news: NotificationSettings,

    #[DbField]
    pub password: Password,

    #[DbField]
    pub phone: Phone,

    #[DbField]
    pub referral: Option<Referral>,

    #[DbField]
    pub session_account_recovery: bool,

    #[DbField]
    pub telemetry: bool,

    #[DbField]
    pub time_format: TimeFormat,

    #[DbField]
    pub two_factor_auth: TwoFa,

    #[DbField]
    pub week_start: WeekStart,

    #[DbField]
    pub welcome: bool,
}

impl From<ApiUserSettings> for UserSettings {
    fn from(value: ApiUserSettings) -> Self {
        Self {
            remote_id: None,
            crash_reports: value.crash_reports,
            date_format: value.date_format.into(),
            density: value.density.into(),
            device_recovery: value.device_recovery,
            early_access: value.early_access,
            email: value.email.into(),
            flags: value.flags.into(),
            hide_side_panel: value.hide_side_panel,
            high_security: value.high_security.into(),
            invoice_text: value.invoice_text,
            locale: value.locale,
            log_auth: value.log_auth.into(),
            news: NotificationSettings(value.news),
            password: value.password.into(),
            phone: value.phone.into(),
            referral: value.referral.map(Into::into),
            session_account_recovery: value.session_account_recovery,
            telemetry: value.telemetry,
            time_format: value.time_format.into(),
            two_factor_auth: value.two_factor_auth.into(),
            week_start: value.week_start.into(),
            welcome: value.welcome,
        }
    }
}
