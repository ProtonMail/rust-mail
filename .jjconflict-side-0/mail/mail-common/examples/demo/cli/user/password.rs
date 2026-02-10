use crate::Result;
use crate::cli::ctx::MailContextExt;
use crate::cli::read;
use clap::Args;
use futures::TryFutureExt;
use proton_account_api::password::state::StateKind;
use proton_mail_common::{MailContext, MailUserContext};
use std::sync::Arc;

/// Change user password.
#[derive(Debug, Args)]
pub struct Cmd {
    username: String,

    #[clap(long)]
    mbp: bool,
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let user_ctx = ctx.get_user_ctx(&self.username).await?;

        self.run_flow(Arc::clone(&user_ctx)).await?;

        user_ctx.force_event_loop_poll().await?;

        Ok(())
    }

    async fn run_flow(&self, user_ctx: Arc<MailUserContext>) -> Result<()> {
        let mut flow = user_ctx.new_password_change_flow().await?;

        loop {
            match flow.kind()? {
                StateKind::Invalid => {
                    bail!("password flow is in invalid state");
                }

                StateKind::WantTfa => {
                    let _ = flow
                        .submit_totp(read("2nd factor")?)
                        .inspect_err(|e| warn!("{e}"))
                        .await;
                }

                StateKind::WantChange => {
                    if self.mbp {
                        let _ = flow
                            .change_mbox_pass(
                                read("current password")?,
                                read("new mailbox password")?,
                            )
                            .inspect_err(|e| warn!("{e}"))
                            .await;
                    } else {
                        let _ = flow
                            .change_pass(read("current password")?, read("new password")?)
                            .inspect_err(|e| warn!("{e}"))
                            .await;
                    }
                }

                StateKind::Complete => {
                    break;
                }
            }
        }

        Ok(())
    }
}
