use crate::{actions::MailActionError, models::CustomSettings};
use proton_action_queue::action::{self as queue, DefaultVersionConverter};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, RunTransaction};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateMobileSignatureEnabledAction {
    enabled: Option<bool>,
}

impl UpdateMobileSignatureEnabledAction {
    pub fn new(enabled: Option<bool>) -> Self {
        Self { enabled }
    }
}

impl queue::Action for UpdateMobileSignatureEnabledAction {
    const TYPE: queue::Type = queue::Type("update_mobile_signature_enabled");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UpdateMobileSignatureEnabledHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

#[derive(Debug)]
pub struct UpdateMobileSignatureEnabledHandler;

impl queue::Handler for UpdateMobileSignatureEnabledHandler {
    type Action = UpdateMobileSignatureEnabledAction;

    async fn apply_local(
        &self,
        _: queue::ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        let mut settings = CustomSettings::get_or_default(tx.tether()).await?;

        settings.mobile_signature_enabled = action.enabled.clone();
        settings.save(tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: queue::ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), MailActionError> {
        // No need to revert, since apply_remote() can't fail
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: queue::ActionId,
        _: &mut Self::Action,
        _: queue::WriterGuard<'_>,
    ) -> Result<(), MailActionError> {
        // This is a purely local setting
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_context::MailTestContext;

    #[tokio::test]
    async fn smoke() {
        let ctx = MailTestContext::new().await;
        let ctx = ctx.uninitialized_mail_user_context().await;

        assert_eq!(
            None,
            CustomSettings::get_or_default(&ctx.user_stash().connection())
                .await
                .unwrap()
                .mobile_signature
        );

        ctx.queue_action(UpdateMobileSignatureEnabledAction::new(Some(true)))
            .await
            .unwrap();

        assert_eq!(
            Some(true),
            CustomSettings::get_or_default(&ctx.user_stash().connection())
                .await
                .unwrap()
                .mobile_signature_enabled
        );
    }
}
