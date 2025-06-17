use crate::datatypes::{
    DateFormat, Density, Email, HighSecurity, LogAuth, Password, Phone, Referral, SettingsFlags,
    TimeFormat, TwoFa, WeekStart,
};
use proton_core_api::services::proton::UserId;
use proton_core_api::services::proton::UserSettings as ApiUserSettings;
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::Bond;
use stash::stash::StashError;

use crate::models::ModelExtension as _;

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
    pub news: u32,

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

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl UserSettings {
    /// Save a user's settings to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, bond).await? {
                self.row_id = existing.row_id;
            }
        }

        <Self as Model>::save(self, bond).await
    }
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
            news: value.news,
            password: value.password.into(),
            phone: value.phone.into(),
            referral: value.referral.map(Into::into),
            session_account_recovery: value.session_account_recovery,
            telemetry: value.telemetry,
            time_format: value.time_format.into(),
            two_factor_auth: value.two_factor_auth.into(),
            week_start: value.week_start.into(),
            welcome: value.welcome,
            row_id: None,
        }
    }
}
