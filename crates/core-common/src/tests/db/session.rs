#![allow(non_snake_case)]

use crate::datatypes::{AuthScope, PasswordMode, RemoteId, TfaStatus};
use crate::db::account::{
    CoreAccount, CoreSession, EncryptedAccessToken, EncryptedRefreshToken, SessionEncryptionKey,
};
use crate::models::ModelExtension;
use proton_api_core::auth::{AuthSession, AuthState, UserKeySecret};
use secrecy::SecretString;
use stash::orm::Model;
use stash::params;
use stash::stash::{Interface, Stash};
use std::io::stdout;
use tracing::subscriber::set_global_default;
use tracing::Level;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{registry, EnvFilter};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

async fn new_test_connection() -> Stash {
    drop(set_global_default(
        registry()
            .with(EnvFilter::new("debug,stash=debug"))
            .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
    ));
    use crate::db::migrations::migrate_account_db;
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_account_db(&stash).await.expect("failed to migrate");
    stash
}

async fn new_test_account(stash: &Stash) -> Result<CoreAccount> {
    let account = CoreAccount::new(
        RemoteId::from("user_id"),
        String::from("name_or_addr"),
        TfaStatus::None,
        PasswordMode::One,
    )
    .with_stash(stash)
    .with_save()
    .await?;

    Ok(account)
}

/// Create a test auth session with dummy data.
fn new_test_auth(account: &CoreAccount) -> AuthSession {
    let uid = RemoteId::from("session_id");
    let user_id = account.remote_id.clone();
    let refresh_token = SecretString::from("token".to_owned());
    let access_token = SecretString::from("access".to_owned());
    let scopes = ["foo".to_owned(), "bar".to_owned()];

    AuthSession {
        uid: uid.into(),
        name_or_addr: account.name_or_addr.clone(),
        user_id: user_id.into(),
        second_factor_mode: TfaStatus::None.into(),
        password_mode: PasswordMode::One.into(),
        access_token: access_token.into(),
        refresh_token: refresh_token.into(),
        auth_scope: scopes.into(),
        auth_state: AuthState::Ready,
    }
}

#[test]
fn test_encryption() {
    let key = SessionEncryptionKey::random();
    let ciphertext = key.encrypt(b"plaintext message".as_ref()).unwrap();
    let plaintext = key.decrypt(&ciphertext).unwrap();
    assert_eq!(&plaintext, b"plaintext message");
}

#[tokio::test]
async fn test_session_store_load() {
    let key = SessionEncryptionKey::random();
    let stash = new_test_connection().await;
    let account = new_test_account(&stash).await.unwrap();
    let auth = new_test_auth(&account);

    let mut session = CoreSession::new(auth, &key).unwrap();

    {
        let tx = stash
            .transaction()
            .await
            .expect("failed to start transaction");

        session
            .save_using(&tx)
            .await
            .expect("failed to store session");

        session.set_stash(&stash);

        let db_session = CoreSession::find_first(
            "WHERE account_id = ?",
            params![session.account_id.clone()],
            &tx,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(session, db_session);

        tx.commit().await
    }
    .expect("failed");
}

#[tokio::test]
async fn test_session_update() {
    let key = SessionEncryptionKey::random();
    let stash = new_test_connection().await;
    let account = new_test_account(&stash).await.unwrap();
    let auth = new_test_auth(&account);

    let mut session = CoreSession::new(auth, &key)
        .unwrap()
        .with_key_secret(&UserKeySecret::from(vec![1, 2, 3, 4]), &key)
        .unwrap();

    {
        let tx = stash
            .transaction()
            .await
            .expect("failed to start transaction");

        session
            .save_using(&tx)
            .await
            .expect("failed to store session");

        // Back up the original session
        let original_session = session.clone();

        // Update the session's tokens and scopes.
        session.access_token = EncryptedAccessToken::new(&"acc".to_owned().into(), &key).unwrap();
        session.refresh_token = EncryptedRefreshToken::new(&"acc".to_owned().into(), &key).unwrap();
        session.auth_scope = AuthScope::new(["baz", "qux"]);
        session.save_using(&tx).await.expect("failed to update");
        session.set_stash(&stash);

        // Load the updated session from the database
        let db_session = CoreSession::find_first(
            "WHERE account_id = ?",
            params![original_session.account_id.clone()],
            &tx,
        )
        .await
        .unwrap()
        .unwrap();

        // This data has changed
        assert_eq!(db_session.access_token, session.access_token);
        assert_eq!(db_session.refresh_token, session.refresh_token);
        assert_eq!(db_session.auth_scope, session.auth_scope);

        // This data is unchanged
        assert_eq!(db_session.remote_id, original_session.remote_id);
        assert_eq!(db_session.account_id, original_session.account_id);
        assert_eq!(db_session.key_secret, original_session.key_secret);

        tx.commit().await
    }
    .expect("failed");
}

#[tokio::test]
async fn test_session_delete_user_id() {
    let key = SessionEncryptionKey::random();
    let stash = new_test_connection().await;
    let account = new_test_account(&stash).await.unwrap();
    let auth = new_test_auth(&account);
    let mut session = CoreSession::new(auth, &key).unwrap();

    {
        let tx = stash
            .transaction()
            .await
            .expect("failed to start transaction");

        session
            .save_using(&tx)
            .await
            .expect("failed to store session");

        session.set_stash(&stash);

        tx.execute(
            "DELETE FROM core_sessions WHERE account_id =?",
            params![session.account_id.clone()],
        )
        .await
        .expect("expect failed to delete user");

        let results = CoreSession::find(
            "WHERE account_id = ?",
            params![session.account_id.clone()],
            &tx,
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 0);
        tx.commit().await
    }
    .expect("failed");
}

#[tokio::test]
async fn test_session_delete_session_id() {
    let key = SessionEncryptionKey::random();
    let stash = new_test_connection().await;
    let account = new_test_account(&stash).await.unwrap();
    let auth = new_test_auth(&account);

    let mut session = CoreSession::new(auth, &key).unwrap();

    {
        let tx = stash
            .transaction()
            .await
            .expect("failed to start transaction");

        session
            .save_using(&tx)
            .await
            .expect("failed to store session");

        session.set_stash(&stash);

        tx.execute(
            "DELETE FROM core_sessions WHERE remote_id =?",
            params![session.remote_id.clone()],
        )
        .await
        .expect("expect failed to delete user");

        let results = CoreSession::find(
            "WHERE account_id = ?",
            params![session.account_id.clone()],
            &tx,
            None,
        )
        .await
        .unwrap();
        assert_eq!(results.len(), 0);
        tx.commit().await
    }
    .expect("failed");
}
