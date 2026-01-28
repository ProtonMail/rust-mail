use std::sync::Arc;

use crate::CoreContextError;
use crate::datatypes::{
    Flags, InitializationKey, ProductUsedSpace, UnixTimestamp, UserKeys, UserMnemonicStatus,
    UserType,
};
use derive_more::TryFrom;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::User as ApiUser;
use proton_core_api::services::proton::UserId;
use proton_core_api::services::proton::{
    DelinquentState as ApiDelinquentState, ProtonCore, Role as ApiRole,
};
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use stash::UserDb;
use stash::exports::{FromSql, FromSqlError, SqliteError, ToSql, ToSqlOutput, Transaction, Value};
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::Stash;
use stash::stash::StashError;

use super::{InitializationError, InitializationWatcher, InitializedComponent, UserSettings};

#[derive(Clone, Debug, Eq, Model, PartialEq, SmartDefault)]
#[TableName("users")]
#[Database(UserDb)]
pub struct User {
    #[IdField(optional)]
    pub remote_id: Option<UserId>,

    #[DbField]
    pub create_time: UnixTimestamp,

    #[DbField]
    pub credit: i64,

    #[DbField]
    pub currency: String,

    #[DbField]
    pub delinquent: DelinquentState,

    #[DbField]
    pub display_name: Option<String>,

    #[DbField]
    pub email: String,

    #[DbField]
    pub keys: UserKeys,

    #[DbField]
    pub flags: Flags,

    #[DbField]
    pub max_space: i64,

    #[DbField]
    pub max_upload: i64,

    #[DbField]
    pub mnemonic_status: UserMnemonicStatus,

    #[DbField]
    pub private: bool,

    #[DbField]
    pub name: Option<String>,

    #[DbField]
    pub product_used_space: ProductUsedSpace,

    #[DbField]
    pub role: Role,

    /// Activated services (bitmap): 1: User has the mail product activated, 4: User has the VPN activated
    /// TODO: Double check that this is up to date
    #[DbField]
    pub services: u32,

    #[DbField]
    pub subscribed: PaidSubscription,

    #[DbField]
    pub to_migrate: bool,

    #[DbField]
    pub used_space: i64,

    #[DbField]
    pub user_type: UserType,
}

impl From<ApiUser> for User {
    fn from(value: ApiUser) -> Self {
        Self {
            remote_id: Some(value.id),
            create_time: value.create_time.into(),
            credit: value.credit,
            currency: value.currency,
            delinquent: value.delinquent.into(),
            display_name: value.display_name,
            email: value.email,
            keys: value.keys.into(),
            flags: value.flags.into(),
            max_space: value.max_space,
            max_upload: value.max_upload,
            mnemonic_status: value.mnemonic_status.into(),
            private: value.private,
            name: value.name,
            product_used_space: value.product_used_space.into(),
            role: value.role.into(),
            services: value.services,
            subscribed: PaidSubscription(value.subscribed),
            to_migrate: value.to_migrate,
            used_space: value.used_space,
            user_type: value.user_type.into(),
        }
    }
}

impl User {
    /// Download and store user info and settings into the database
    ///
    pub async fn sync_user_and_settings(
        api: &impl ProtonCore,
    ) -> Result<SyncedUserSettings, ApiServiceError> {
        let user = User::from(api.get_users().await?.user);
        let mut settings = UserSettings::from(api.get_settings().await?.user_settings);
        settings.remote_id.clone_from(&user.remote_id);

        Ok(SyncedUserSettings { user, settings })
    }

    pub const INIT_KEY: InitializationKey = InitializationKey::new("user_settings");

    /// It initializes user and settings by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize_with_settings<API>(
        watcher: Arc<InitializationWatcher>,
        api: &API,
        stash: &Stash<UserDb>,
    ) -> Result<(), InitializationError<CoreContextError>>
    where
        API: ProtonCore,
    {
        InitializedComponent::initialize(
            watcher,
            Self::INIT_KEY,
            &[],
            stash.connection().await?,
            async move || Ok(Self::sync_user_and_settings(api).await?),
            |tx, res| {
                res.store(tx)?;
                Ok(())
            },
        )
        .await
    }

    #[must_use]
    pub fn is_delinquent(&self) -> bool {
        self.delinquent != DelinquentState::Paid
    }

    #[must_use]
    pub fn with_paid_mail_plan(mut self) -> Self {
        self.subscribed.insert(PaidSubscription::MAIL);
        self
    }

    #[must_use]
    pub fn has_paid_mail_plan(&self) -> bool {
        self.subscribed.contains(PaidSubscription::MAIL) && !self.is_delinquent()
    }
}

#[must_use]
#[derive(Debug)]
pub struct SyncedUserSettings {
    user: User,
    settings: UserSettings,
}

impl SyncedUserSettings {
    #[tracing::instrument(skip(tx))]
    pub fn store(self, tx: &Transaction<'_>) -> Result<(), StashError> {
        let Self {
            mut user,
            mut settings,
        } = self;
        user.save_sync(tx)?;
        settings.save_sync(tx)?;
        Ok(())
    }
}

/// What services a user has subscribed to
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[repr(transparent)]
pub struct PaidSubscription(pub u32);

impl FromSql for PaidSubscription {
    fn column_result(value: stash::exports::ValueRef<'_>) -> stash::exports::FromSqlResult<Self> {
        let val = u32::column_result(value)?;
        Ok(Self(val))
    }
}

impl ToSql for PaidSubscription {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(self.0.into())))
    }
}

bitflags::bitflags! {
    impl PaidSubscription:u32 {
        const MAIL = 1 << 0;
        const DRIVE = 1 << 1;
        const VPN = 1 << 2;
        const PASS = 1 << 3;
        const WALLET = 1 << 4;
        const NEUTRON = 1 << 5;
        const LUMO = 1 << 6;
        const AUTHENTICATOR = 1 << 7;
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize, TryFrom)]
#[try_from(repr)]
#[repr(u32)]
pub enum Role {
    #[default]
    None = 0,
    Member = 1,
    Admin = 2,
    Unknown(u32),
}

impl From<Role> for i64 {
    fn from(value: Role) -> Self {
        match value {
            Role::None => 0,
            Role::Member => 1,
            Role::Admin => 2,
            Role::Unknown(v) => i64::from(v),
        }
    }
}

impl From<ApiRole> for Role {
    fn from(value: ApiRole) -> Self {
        match value {
            ApiRole::None => Self::None,
            ApiRole::Member => Self::Member,
            ApiRole::Admin => Self::Admin,
            ApiRole::Unknown(v) => Self::Unknown(v),
        }
    }
}

impl FromSql for Role {
    fn column_result(value: stash::exports::ValueRef<'_>) -> stash::exports::FromSqlResult<Self> {
        let val = u32::column_result(value)?;
        Ok(Self::try_from(val).unwrap_or(Role::Unknown(val)))
    }
}

impl ToSql for Role {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer((*self).into())))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize, TryFrom)]
#[try_from(repr)]
#[repr(u32)]
pub enum DelinquentState {
    #[default]
    /// The user's account is fully paid.
    Paid = 0,
    /// The user's account is available but not yet paid.
    Available = 1,
    /// The user's account has an overdue payment.
    Overdue = 2,
    /// The user's account is delinquent due to unpaid dues.
    Delinquent = 3,
    /// The user's payment has not been received.
    NotReceived = 4,
}

impl From<ApiDelinquentState> for DelinquentState {
    fn from(value: ApiDelinquentState) -> Self {
        match value {
            ApiDelinquentState::Paid => DelinquentState::Paid,
            ApiDelinquentState::Available => DelinquentState::Available,
            ApiDelinquentState::Overdue => DelinquentState::Overdue,
            ApiDelinquentState::Delinquent => DelinquentState::Delinquent,
            ApiDelinquentState::NotReceived => DelinquentState::NotReceived,
        }
    }
}

impl FromSql for DelinquentState {
    fn column_result(value: stash::exports::ValueRef<'_>) -> stash::exports::FromSqlResult<Self> {
        let val = u32::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for DelinquentState {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}
