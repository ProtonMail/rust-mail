use crate::datatypes::NotAMagicLocalIdError;
use crate::draft::compose::PM_SIGNATURE;
use crate::migration_snooper::PostLoginMobileMigrationPayload;
use crate::{AppError, MailUserContext};
use proton_core_api::services::proton::UserId;
use proton_core_common::models::{InitializationError, InitializationWatcher, User};
use proton_core_common::{datatypes::InitializationKey, models::InitializedComponent};
use stash::AccountDb;
use stash::exports::{
    FromSql, FromSqlError, FromSqlResult, SqliteError, ToSql, ToSqlOutput, Transaction, Value,
    ValueRef,
};
use stash::macros::Model;
use stash::orm::Model;
use stash::stash::{RunTransaction, Stash, StashError, Tether};
use std::sync::Arc;
use tracing::instrument;

#[derive(Clone, Debug, Default, Eq, Model, PartialEq)]
#[TableName("custom_settings")]
pub struct CustomSettings {
    #[IdField]
    pub local_id: CustomSettingsId,

    #[DbField]
    pub address_signature_enabled: Option<bool>,

    #[DbField]
    pub mobile_signature: Option<String>,

    #[DbField]
    pub mobile_signature_enabled: Option<bool>,

    #[DbField]
    pub swipe_to_adjacent_conversation: Option<bool>,
}

impl CustomSettings {
    pub const INIT_KEY: InitializationKey = InitializationKey::new("custom_settings");

    #[instrument(skip_all)]
    pub async fn initialize(
        watcher: Arc<InitializationWatcher>,
        user_id: &UserId,
        user_stash: &Stash,
        account_stash: &Stash<AccountDb>,
    ) -> Result<(), InitializationError<AppError>> {
        let mut this = Self::default();

        let payload = account_stash
            .connection()
            .await?
            .tx(async |tx| PostLoginMobileMigrationPayload::load(user_id, tx).await)
            .await?;

        if let Some(payload) = payload {
            this.address_signature_enabled = payload.address_signature_enabled;

            this.mobile_signature = payload
                .mobile_signature
                .map(|sig| Self::sanitize_mobile_signature(&sig));

            this.mobile_signature_enabled = payload.mobile_signature_enabled;
        }

        InitializedComponent::initialize(
            watcher,
            Self::INIT_KEY,
            &[],
            user_stash.connection().await?,
            async move || Ok(SyncedCustomSettings { settings: this }),
            |tx, synced| {
                synced.store(tx)?;
                Ok(())
            },
        )
        .await
    }

    #[instrument(skip_all)]
    pub async fn get(tether: &Tether) -> Result<Option<Self>, StashError> {
        Self::load(CustomSettingsId, tether).await
    }

    #[instrument(skip_all)]
    pub async fn get_or_default(tether: &Tether) -> Result<Self, StashError> {
        Ok(Self::get(tether).await?.unwrap_or_default())
    }

    #[must_use]
    pub fn address_signature_enabled(&self) -> bool {
        self.address_signature_enabled.unwrap_or(true)
    }

    #[must_use]
    pub fn with_address_signature_enabled(mut self, address_signature_enabled: bool) -> Self {
        self.address_signature_enabled = Some(address_signature_enabled);
        self
    }

    #[must_use]
    pub fn mobile_signature(&self) -> &str {
        self.mobile_signature.as_deref().unwrap_or(PM_SIGNATURE)
    }

    #[must_use]
    pub fn with_mobile_signature(mut self, mobile_signature: &str) -> Self {
        self.mobile_signature = Some(mobile_signature.into());
        self
    }

    #[instrument(skip_all)]
    pub async fn update_mobile_signature(
        ctx: &MailUserContext,
        signature: Option<String>,
    ) -> Result<(), StashError> {
        ctx.user_stash()
            .connection()
            .await?
            .tx(async move |tx| {
                let mut this = Self::get_or_default(tx.tether()).await?;

                this.mobile_signature = signature.map(|sig| Self::sanitize_mobile_signature(&sig));
                this.save(tx).await?;

                Ok(())
            })
            .await
    }

    #[must_use]
    pub fn with_mobile_signature_enabled(mut self, mobile_signature_enabled: bool) -> Self {
        self.mobile_signature_enabled = Some(mobile_signature_enabled);
        self
    }

    #[must_use]
    pub fn with_swipe_to_adjacent_conversation(
        mut self,
        swipe_to_adjacent_conversation: bool,
    ) -> Self {
        self.swipe_to_adjacent_conversation = Some(swipe_to_adjacent_conversation);
        self
    }

    #[must_use]
    pub fn mobile_signature_enabled(&self) -> bool {
        self.mobile_signature_enabled.unwrap_or(true)
    }

    #[must_use]
    #[instrument(skip_all)]
    pub fn swipe_to_adjacent_conversation(&self) -> bool {
        self.swipe_to_adjacent_conversation.unwrap_or(true)
    }

    #[instrument(skip_all)]
    pub async fn update_mobile_signature_enabled(
        ctx: &MailUserContext,
        enabled: Option<bool>,
    ) -> Result<(), StashError> {
        ctx.user_stash()
            .connection()
            .await?
            .tx(async move |tx| {
                let mut this = CustomSettings::get_or_default(tx.tether()).await?;

                this.mobile_signature_enabled = enabled;
                this.save(tx).await?;

                Ok(())
            })
            .await
    }

    #[instrument(skip_all)]
    pub async fn update_swipe_to_adjacent_conversation(
        ctx: &MailUserContext,
        enabled: Option<bool>,
    ) -> Result<(), StashError> {
        ctx.user_stash()
            .connection()
            .await?
            .tx(async move |tx| {
                let mut this = CustomSettings::get_or_default(tx.tether()).await?;

                this.swipe_to_adjacent_conversation = enabled;
                this.save(tx).await?;

                Ok(())
            })
            .await
    }

    /// From user's perspective inputting newlines is easier than writing HTML
    /// tags, so for better UX we convert plain newlines into "HTML newlines".
    ///
    /// TODO(ET-4743): Remove when the mobile signature editor becomes WYSIWYG
    fn sanitize_mobile_signature(sig: &str) -> String {
        sig.trim().replace("\r\n", "<br />").replace("\n", "<br />")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct CustomSettingsId;

impl CustomSettingsId {
    const MAGIC_ID: u32 = 1;
}

impl FromSql for CustomSettingsId {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        let got = u32::from(u8::column_result(value)?);

        if got == Self::MAGIC_ID {
            Ok(Self)
        } else {
            Err(FromSqlError::Other(Box::new(NotAMagicLocalIdError {
                expected: Self::MAGIC_ID,
                got,
            })))
        }
    }
}

impl ToSql for CustomSettingsId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(i64::from(
            Self::MAGIC_ID,
        ))))
    }
}

#[must_use]
#[derive(Debug)]
pub struct SyncedCustomSettings {
    settings: CustomSettings,
}

impl SyncedCustomSettings {
    #[tracing::instrument(skip_all)]
    pub fn store(mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        self.settings.save_sync(tx)?;

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum MobileSignatureStatus {
    Enabled,
    Disabled,
    NeedsPaidVersion,
}

impl MobileSignatureStatus {
    pub fn new(user: &User, settings: &CustomSettings) -> Self {
        if user.has_paid_mail_plan() {
            if settings.mobile_signature_enabled() {
                Self::Enabled
            } else {
                Self::Disabled
            }
        } else {
            Self::NeedsPaidVersion
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_context::MailTestContext;

    #[tokio::test]
    async fn update_mobile_signature() {
        let ctx = MailTestContext::new().await;
        let ctx = ctx.uninitialized_mail_user_context().await;

        assert_eq!(
            None,
            CustomSettings::get_or_default(&ctx.user_stash().connection().await.unwrap())
                .await
                .unwrap()
                .mobile_signature
        );

        CustomSettings::update_mobile_signature(
            &ctx,
            Some("greetings from my oxidized mail,\ncheers\n".into()),
        )
        .await
        .unwrap();

        assert_eq!(
            Some("greetings from my oxidized mail,<br />cheers".into()),
            CustomSettings::get_or_default(&ctx.user_stash().connection().await.unwrap())
                .await
                .unwrap()
                .mobile_signature
        );
    }

    /// Mobile signature used to be stored in a `STRING` column by accident - in
    /// SQLite strings have numeric affinity and, long story short, this used to
    /// fail, because `"1234"` (a string) was being interpreted as `1234` (an
    /// integer).
    #[tokio::test]
    async fn update_mobile_signature_with_just_digits() {
        let ctx = MailTestContext::new().await;
        let ctx = ctx.uninitialized_mail_user_context().await;

        CustomSettings::update_mobile_signature(&ctx, Some("1234".into()))
            .await
            .unwrap();

        assert_eq!(
            Some("1234".into()),
            CustomSettings::get_or_default(&ctx.user_stash().connection().await.unwrap())
                .await
                .unwrap()
                .mobile_signature
        );
    }

    #[tokio::test]
    async fn update_mobile_signature_enabled() {
        let ctx = MailTestContext::new().await;
        let ctx = ctx.uninitialized_mail_user_context().await;

        assert_eq!(
            None,
            CustomSettings::get_or_default(&ctx.user_stash().connection().await.unwrap())
                .await
                .unwrap()
                .mobile_signature
        );

        CustomSettings::update_mobile_signature_enabled(&ctx, Some(true))
            .await
            .unwrap();

        assert_eq!(
            Some(true),
            CustomSettings::get_or_default(&ctx.user_stash().connection().await.unwrap())
                .await
                .unwrap()
                .mobile_signature_enabled
        );
    }
}
