use crate::cli::ctx::MailContextExt;
use crate::cli::read;
use anyhow::Result;
use futures::TryFutureExt;
use proton_account_api::login::state::want_qr_confirmation::process_target_device_qr_code;
use proton_core_api::services::observability::ObservabilityRecorder;
use proton_mail_common::MailContext;
use std::sync::Arc;
use std::time::Duration;

/// Initiates a login flow, generates a QR code for user authentication,
/// and polls for confirmation from a host device.
#[derive(Debug, Args)]
pub struct TargetCmd {}

impl TargetCmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let mut flow = ctx.new_or_resume_login_flow(None).await?;

        let qr = flow.generate_sign_in_qr_code(true).await?;
        println!("QR: {qr}");
        let _ = read("Copy the QR code and press enter when the Host device confirmed the login")?;

        assert!(flow.is_awaiting_host_device_confirmation());

        loop {
            if let Err(_e) = flow.check_host_device_confirmation().await {
                break;
            } else if flow.is_awaiting_host_device_confirmation() {
                // No confirmation yet, keep polling
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else {
                break;
            }
        }

        assert!(flow.is_logged_in());

        _ = ctx
            .user_context_from_login_flow(&mut flow)
            .inspect_err(|err| error!("failed to create user context: {err:?}"))
            .await?;

        Ok(())
    }
}

/// Initiates a login flow for the specified user, processes a QR code
/// provided by the Target Device, and confirms the authentication.
#[derive(Debug, Args)]
pub struct HostCmd {
    username: String,
}

impl HostCmd {
    pub async fn run(self, mail_ctx: Arc<MailContext>) -> Result<()> {
        let (_login_flow, user_ctx) =
            crate::cli::user::login::Cmd::login(Arc::clone(&mail_ctx), &self.username)
                .await
                .unwrap();

        let ctx = Arc::clone(mail_ctx.core_context());
        let session = user_ctx.session().to_owned();
        let (client, _) = session.into_parts();
        let qr_code = read("QR Code").unwrap();
        process_target_device_qr_code(&qr_code, client, ctx, ObservabilityRecorder::default())
            .await
            .unwrap();
        info!("QR Code successfully confirmed, the Target Device can proceed");

        Ok(())
    }
}
