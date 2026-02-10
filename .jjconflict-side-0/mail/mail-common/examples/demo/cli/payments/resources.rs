use anyhow::Result;
use proton_core_api::services::proton::ProtonPayments;
use proton_mail_common::MailUserContext;
use std::sync::Arc;

/// Manage payments resources.
#[derive(Debug, Subcommand)]
pub enum Cmd {
    /// Dump the icon for the given resource as a PNG file.
    Icon(PaymentsResourcesIconCmd),
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailUserContext>) -> Result<()> {
        match self {
            Self::Icon(cmd) => cmd.run(ctx).await,
        }
    }
}

#[derive(Debug, Args)]
pub struct PaymentsResourcesIconCmd {
    name: String,
}

impl PaymentsResourcesIconCmd {
    async fn run(self, ctx: Arc<MailUserContext>) -> Result<()> {
        let icon = ctx
            .session()
            .get_payments_resources_icons(self.name.clone())
            .await?;

        std::fs::write(format!("{}.png", self.name), icon)?;

        Ok(())
    }
}
