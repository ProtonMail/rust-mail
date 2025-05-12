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

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("user_settings")]
#[allow(clippy::struct_excessive_bools)]
pub struct UserSettings {
    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[IdField(optional)]
    pub remote_id: Option<UserId>,

    /// TODO: Document this field.
    #[DbField]
    pub crash_reports: bool,

    /// TODO: Document this field.
    #[DbField]
    pub date_format: DateFormat,

    /// TODO: Document this field.
    #[DbField]
    pub density: Density,

    /// TODO: Document this field.
    #[DbField]
    pub device_recovery: bool,

    /// TODO: Document this field.
    #[DbField]
    pub early_access: bool,

    /// TODO: Document this field.
    #[DbField]
    pub email: Email,

    /// TODO: Document this field.
    #[DbField]
    pub flags: SettingsFlags,

    /// TODO: Document this field.
    #[DbField]
    pub hide_side_panel: bool,

    /// TODO: Document this field.
    #[DbField]
    pub high_security: HighSecurity,

    /// TODO: Document this field.
    #[DbField]
    pub invoice_text: String,

    /// TODO: Document this field.
    #[DbField]
    pub locale: String,

    /// TODO: Document this field.
    #[DbField]
    pub log_auth: LogAuth,

    /// TODO: Document this field.
    #[DbField]
    pub news: u32,

    /// TODO: Document this field.
    #[DbField]
    pub password: Password,

    /// TODO: Document this field.
    #[DbField]
    pub phone: Phone,

    /// TODO: Document this field.
    #[DbField]
    pub referral: Option<Referral>,

    /// TODO: Document this field.
    #[DbField]
    pub session_account_recovery: bool,

    /// TODO: Document this field.
    #[DbField]
    pub telemetry: bool,

    /// TODO: Document this field.
    #[DbField]
    pub time_format: TimeFormat,

    /// TODO: Document this field.
    #[DbField]
    pub two_factor_auth: TwoFa,

    /// TODO: Document this field.
    #[DbField]
    pub week_start: WeekStart,

    /// TODO: Document this field.
    #[DbField]
    pub welcome: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl UserSettings {
    /// Save a user's settings to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the records.
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
