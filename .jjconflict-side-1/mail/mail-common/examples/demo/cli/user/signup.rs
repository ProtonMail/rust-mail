use crate::cli::read;
use anyhow::Result;
use proton_account_api::shared::challenge::Behavior;
use proton_account_api::signup::state::StateKind;
use proton_account_api::signup::{SignupError, SignupFlow};
use proton_mail_common::MailContext;
use std::sync::Arc;

/// Signup for an account.
#[derive(Debug, Args)]
pub struct Cmd {
    // ...
}

impl Cmd {
    pub async fn run(self, ctx: Arc<MailContext>) -> Result<()> {
        let mut flow = ctx.new_signup_flow().await?;

        assert!(matches!(flow.kind()?, StateKind::WantUsername));
        Self::on_want_username(&mut flow).await?;

        assert!(matches!(flow.kind()?, StateKind::WantPassword));
        Self::on_want_password(&mut flow)?;

        assert!(matches!(flow.kind()?, StateKind::WantRecovery));
        Self::on_want_recovery(&mut flow).await?;

        assert!(matches!(flow.kind()?, StateKind::WantCreate));
        Self::on_want_create(&mut flow).await?;

        assert!(matches!(flow.kind()?, StateKind::Complete));
        Self::on_complete(&mut flow)?;

        Ok(())
    }

    async fn on_want_username(flow: &mut SignupFlow) -> Result<()> {
        println!("Available domains:");

        for domain in flow.available_domains() {
            println!("  - {domain}");
        }

        loop {
            let username = read("username")?;
            let domain = read("domain")?;
            let behaviour = new_behaviour(&username);

            match flow
                .submit_internal_username(username, domain, Some(behaviour))
                .await
            {
                Err(SignupError::UsernameUnavailable(msg)) => {
                    println!("Username unavailable, try again (error: {msg:?})");
                }

                Ok(()) => return Ok(()),
                Err(e) => return Err(e)?,
            }
        }
    }

    fn on_want_password(flow: &mut SignupFlow) -> Result<()> {
        let password = read("password")?;

        flow.submit_password(password)?;

        Ok(())
    }

    async fn on_want_recovery(flow: &mut SignupFlow) -> Result<()> {
        flow.skip_recovery(None).await?;

        Ok(())
    }

    async fn on_want_create(flow: &mut SignupFlow) -> Result<()> {
        flow.create().await?;

        Ok(())
    }

    fn on_complete(flow: &mut SignupFlow) -> Result<()> {
        let (_, user, addr) = flow.complete()?;

        println!("{user:#?}");
        println!("{addr:#?}");

        Ok(())
    }
}

fn new_behaviour(username: &str) -> Behavior {
    Behavior {
        time: vec![6],
        click: 0,
        copy: vec![],
        paste: vec![],
        keydown: username.chars().map(|c| c.to_string()).collect(),
    }
}
