use proton_api_core::services::proton::Config;
use proton_core_common::datatypes::RemoteId;
use proton_core_common::db::account::SessionEncryptionKey;
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_core_common::{CoreAccountState, CoreSessionState};
use proton_mail_common::MailContext;
use std::error::Error;
use std::sync::Arc;
use tempdir::TempDir;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    FmtSubscriber::builder().with_max_level(Level::DEBUG).init();

    let dir = TempDir::new("data")?;
    let sessions = dir.path().join("sessions");
    let users = dir.path().join("users");
    let cache = dir.path().join("cache");

    let key = SessionEncryptionKey::random().to_base64();
    let kch = InMemoryKeyChain::default();
    kch.store(key)?;

    let cfg = Config::default();
    let kch = Arc::new(kch);
    let ctx = MailContext::new(sessions, users, cache, 1 << 20, kch, cfg, None).await?;

    // Login first account
    let (user_id_1, session_id_1): (RemoteId, RemoteId) = {
        let mut flow = ctx.new_login_flow().await?;

        let username = std::env::var("TEST_USERNAME_1")?;
        let password = std::env::var("TEST_PASSWORD_1")?;

        flow.login(username, password, None).await?;

        let user_id = flow.user_id()?;
        let session_id = flow.session_id()?;

        (user_id.to_owned().into(), session_id.to_owned().into())
    };

    // Check we can create the user context
    {
        let session = ctx.get_session(session_id_1.clone()).await?.unwrap();
        let context = ctx.user_context_from_session(&session).await?;

        assert_eq!(context.user_id(), &user_id_1);
        assert_eq!(context.session_id(), &session_id_1);
    }

    // 1 account with 1 session
    {
        assert_eq!(ctx.get_accounts().await?.len(), 1);
        assert_eq!(ctx.get_sessions(user_id_1.clone()).await?.len(), 1);
    }

    // First account is logged in and session is ready
    {
        assert!(matches!(
            ctx.get_account_state(user_id_1.clone()).await?,
            Some(CoreAccountState::LoggedIn(_)),
        ));

        assert!(matches!(
            ctx.get_session_state(session_id_1.clone()).await?,
            Some(CoreSessionState::Ready),
        ));
    }

    // First account is the primary account
    {
        let account = ctx.get_primary_account().await?;
        let user_id = account.unwrap().remote_id;

        assert_eq!(user_id, user_id_1);
    }

    // Logout the first account's session
    {
        let session = ctx.get_session(session_id_1.clone()).await?.unwrap();
        let context = ctx.user_context_from_session(&session).await?;

        context.logout().await?;
    }

    // 1 account with 0 sessions
    {
        assert_eq!(ctx.get_accounts().await?.len(), 1);
        assert_eq!(ctx.get_sessions(user_id_1.clone()).await?.len(), 0);
    }

    // Account is logged out
    {
        assert!(matches!(
            ctx.get_account_state(user_id_1.clone()).await?,
            Some(CoreAccountState::LoggedOut),
        ));

        assert!(matches!(
            ctx.get_session_state(session_id_1.clone()).await?,
            None,
        ));
    }

    // Login second account
    let (user_id_2, session_id_2): (RemoteId, RemoteId) = {
        let mut flow = ctx.new_login_flow().await?;

        let username = std::env::var("TEST_USERNAME_2")?;
        let password = std::env::var("TEST_PASSWORD_2")?;

        flow.login(username, password, None).await?;

        let user_id = flow.user_id()?;
        let session_id = flow.session_id()?;

        (user_id.to_owned().into(), session_id.to_owned().into())
    };

    // Check we can create the user context
    {
        let session = ctx.get_session(session_id_2.clone()).await?.unwrap();
        let context = ctx.user_context_from_session(&session).await?;

        assert_eq!(context.user_id(), &user_id_2);
        assert_eq!(context.session_id(), &session_id_2);
    }

    // 2 accounts with 0 and 1 session respectively
    {
        assert_eq!(ctx.get_accounts().await?.len(), 2);
        assert_eq!(ctx.get_sessions(user_id_1.clone()).await?.len(), 0);
        assert_eq!(ctx.get_sessions(user_id_2.clone()).await?.len(), 1);
    }

    // One account is logged in, the other is logged out
    {
        assert!(matches!(
            ctx.get_account_state(user_id_1.clone()).await?,
            Some(CoreAccountState::LoggedOut),
        ));

        assert!(matches!(
            ctx.get_session_state(session_id_1.clone()).await?,
            None,
        ));

        assert!(matches!(
            ctx.get_account_state(user_id_2.clone()).await?,
            Some(CoreAccountState::LoggedIn(_)),
        ));

        assert!(matches!(
            ctx.get_session_state(session_id_2.clone()).await?,
            Some(CoreSessionState::Ready),
        ));
    }

    // Make the second account primary
    {
        ctx.set_primary_account(user_id_2.clone()).await?;
    }

    // Second account is the primary account
    {
        let account = ctx.get_primary_account().await?;
        let user_id = account.unwrap().remote_id;

        assert_eq!(user_id, user_id_2);
    }

    // Logout the second account's session
    {
        let session = ctx.get_session(session_id_2.clone()).await?.unwrap();
        let context = ctx.user_context_from_session(&session).await?;

        context.logout().await?;
    }

    // 2 accounts with 0 sessions each
    {
        assert_eq!(ctx.get_accounts().await?.len(), 2);
        assert_eq!(ctx.get_sessions(user_id_1.clone()).await?.len(), 0);
        assert_eq!(ctx.get_sessions(user_id_2.clone()).await?.len(), 0);
    }

    // Both accounts are logged out
    {
        assert!(matches!(
            ctx.get_account_state(user_id_1.clone()).await?,
            Some(CoreAccountState::LoggedOut),
        ));

        assert!(matches!(
            ctx.get_session_state(session_id_1.clone()).await?,
            None,
        ));

        assert!(matches!(
            ctx.get_account_state(user_id_2.clone()).await?,
            Some(CoreAccountState::LoggedOut),
        ));

        assert!(matches!(
            ctx.get_session_state(session_id_2.clone()).await?,
            None,
        ));
    }

    // Delete the first account
    {
        ctx.delete_account(user_id_1.clone()).await?;

        assert_eq!(ctx.get_accounts().await?.len(), 1);
    }

    // Delete the second account
    {
        ctx.delete_account(user_id_2.clone()).await?;

        assert_eq!(ctx.get_accounts().await?.len(), 0);
    }

    // Partially login the third account
    let (user_id_3, session_id_3): (RemoteId, RemoteId) = {
        let mut flow = ctx.new_login_flow().await?;

        let username = std::env::var("TEST_USERNAME_3")?;
        let password = std::env::var("TEST_PASSWORD_3")?;

        flow.login(username, password, None).await?;

        let user_id = flow.user_id()?;
        let session_id = flow.session_id()?;

        (user_id.to_owned().into(), session_id.to_owned().into())
    };

    // 1 account with 1 session
    {
        assert_eq!(ctx.get_accounts().await?.len(), 1);
        assert_eq!(ctx.get_sessions(user_id_3.clone()).await?.len(), 1);
    }

    // Third account is awaiting 2FA
    {
        assert!(matches!(
            ctx.get_account_state(user_id_3.clone()).await?,
            Some(CoreAccountState::NeedTfa(_)),
        ));

        assert!(matches!(
            ctx.get_session_state(session_id_3.clone()).await?,
            Some(CoreSessionState::NeedTfa),
        ));
    }

    // Complete the login of the third account
    let (user_id_3, session_id_3): (RemoteId, RemoteId) = {
        let mut flow = ctx.resume_login_flow(user_id_3, session_id_3).await?;

        let tfa_code = std::env::var("TEST_2FA_CODE_3")?;
        let password = std::env::var("TEST_PASSWORD_3")?;

        flow.submit_totp(tfa_code).await?;
        flow.submit_mailbox_password(&password).await?;

        let user_id = flow.user_id()?;
        let session_id = flow.session_id()?;

        (user_id.to_owned().into(), session_id.to_owned().into())
    };

    // Third account is logged in
    {
        assert!(matches!(
            ctx.get_account_state(user_id_3.clone()).await?,
            Some(CoreAccountState::LoggedIn(_)),
        ));

        assert!(matches!(
            ctx.get_session_state(session_id_3.clone()).await?,
            Some(CoreSessionState::Ready),
        ));
    }

    // Logout the third account's session
    {
        let session = ctx.get_session(session_id_3.clone()).await?.unwrap();
        let context = ctx.user_context_from_session(&session).await?;

        context.logout().await?;
    }

    // Delete the third account
    {
        ctx.delete_account(user_id_3.clone()).await?;

        assert_eq!(ctx.get_accounts().await?.len(), 0);
    }

    Ok(())
}
