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
use proton_core_api::services::proton::{DelinquentState as ApiDelinquentState, ProtonCore};
use serde::{Deserialize, Serialize};
use stash::exports::{FromSql, FromSqlError, SqliteError, ToSql, ToSqlOutput, Value};
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::StashError;
use stash::stash::{Bond, Stash};

use super::{
    InitializationError, InitializationWatcher, InitializedComponent, ModelExtension as _,
    UserSettings,
};

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("users")]
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
    pub role: u32,

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

    #[RowIdField]
    pub row_id: Option<u64>,
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
            role: value.role,
            services: value.services,
            subscribed: PaidSubscription(value.subscribed),
            to_migrate: value.to_migrate,
            used_space: value.used_space,
            user_type: value.user_type.into(),
            row_id: None,
        }
    }
}

impl User {
    // /// Get the user's display name.
    // #[must_use]
    // pub fn user_name(&self) -> &str {
    //     if let Some(display_name) = self.display_name.as_deref() {
    //         display_name
    //     } else if let Some(name) = self.name.as_deref() {
    //         name
    //     } else {
    //         &self.email
    //     }
    // }

    /// Save a user to the database.
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

    /// Download and store user info and settings into the database
    ///
    /// # Errors
    ///
    /// TODO: Document the errors.
    ///
    pub async fn sync_user_and_settings(
        api: &impl ProtonCore,
    ) -> Result<SyncedUserSettings, ApiServiceError> {
        let user = User::from(api.get_users().await?.user);
        let mut settings = UserSettings::from(api.get_settings().await?.user_settings);
        settings.remote_id.clone_from(&user.remote_id);

        Ok(SyncedUserSettings { user, settings })
    }

    /// Key used to distinguish between components in the initialization.
    /// It is a string, not an enum for making it open for additional changes from different BU.
    ///
    pub const INIT_KEY: InitializationKey = InitializationKey::new("user_settings");

    /// It initializes user and settings by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize_with_settings<API>(
        watcher: Arc<InitializationWatcher>,
        api: &API,
        stash: &Stash,
    ) -> Result<(), InitializationError<CoreContextError>>
    where
        API: ProtonCore,
    {
        InitializedComponent::initialize::<CoreContextError, SyncedUserSettings>(
            watcher,
            Self::INIT_KEY,
            &[],
            stash.connection(),
            async move || Ok(Self::sync_user_and_settings(api).await?),
            async |tx, res| {
                res.store(tx).await?;
                Ok(())
            },
        )
        .await
    }

    #[must_use]
    pub fn is_deliquent(&self) -> bool {
        self.delinquent != DelinquentState::Paid
    }
}

/// This is a manual implementation of `User::sync_user_and_settings` async closure.
///
/// We keep it as it is until Rust allows us to use `impl Trait` in generics etc.
#[must_use]
#[derive(Debug)]
pub struct SyncedUserSettings {
    user: User,
    settings: UserSettings,
}

impl SyncedUserSettings {
    /// Consume this manual closure by storing data in the Database.
    ///
    #[tracing::instrument(skip(tx))]
    pub async fn store(self, tx: &Bond<'_>) -> Result<(), StashError> {
        let Self {
            mut user,
            mut settings,
        } = self;
        user.save(tx).await?;
        settings.save(tx).await?;
        Ok(())
    }
}

/// What services a user has subscribed to
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, debug_assertions), derive(Serialize))]
#[serde(transparent)]
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
