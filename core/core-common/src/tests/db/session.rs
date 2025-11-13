use crate::datatypes::AuthScopes;
use crate::db::account::{
    CoreAccount, CoreSession, CoreSessionObserver, CoreSessionObserverNotification,
    EncryptedAccessToken, EncryptedRefreshToken, SessionEncryptionKey,
};
use crate::db::migrations::migrate_account_db;
use crate::models::ModelExtension;
use proton_core_api::auth::{Tokens, UserKeySecret};
use proton_core_api::services::proton::{SessionId, UserId};
use secrecy::{ExposeSecret, SecretString};
use stash::orm::Model;
use stash::params;
use stash::stash::{Stash, StashConfiguration, StashError, Tether};
use std::time::Duration;
use tracing::subscriber::set_global_default;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, registry};

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

async fn new_test_connection() -> Stash {
    _ = set_global_default(
        registry()
            .with(EnvFilter::new("debug"))
            .with(layer().with_test_writer()),
    );

    let stash = Stash::new(StashConfiguration::test()).unwrap();

    migrate_account_db(&stash).await.unwrap();

    stash
}

async fn new_test_account(tether: &mut Tether) -> Result<CoreAccount> {
    Ok(tether
        .tx(async |tx| {
            CoreAccount::new(UserId::from("user_id"), String::from("name_or_addr"))
                .with_save(tx)
                .await
        })
        .await?)
}

/// Create test auth tokens with dummy data.
fn new_test_tokens() -> Tokens {
    let refresh_token = SecretString::from("token".to_owned());
    let access_token = SecretString::from("access".to_owned());
    let scopes = ["foo".to_owned(), "bar".to_owned()];

    Tokens::access(
        access_token.expose_secret(),
        refresh_token.expose_secret(),
        scopes,
    )
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
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let account = new_test_account(&mut tether).await.unwrap();
    let session_id = SessionId::from("remote_id");
    let tokens = new_test_tokens();

    let mut session = CoreSession::new(account.remote_id, session_id, &tokens, &key).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            session.save(tx).await.expect("failed to store session");

            let db_session = CoreSession::find_first(
                "WHERE account_id = ?",
                params![session.account_id.clone()],
                tx,
            )
            .await
            .unwrap()
            .unwrap();

            assert_eq!(session, db_session);
            Ok(())
        })
        .await
        .expect("failed");
}

#[tokio::test]
async fn test_session_update() {
    let key = SessionEncryptionKey::random();
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let account = new_test_account(&mut tether).await.unwrap();
    let session_id = SessionId::from("remote_id");
    let tokens = new_test_tokens();

    let mut session = CoreSession::new(account.remote_id, session_id, &tokens, &key)
        .unwrap()
        .with_key_secret(&UserKeySecret::from(vec![1, 2, 3, 4]), &key)
        .unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            session.save(tx).await.expect("failed to store session");

            // Back up the original session
            let original_session = session.clone();

            // Update the session's tokens and scopes.
            session.access_token = EncryptedAccessToken::new("acc", &key).unwrap();
            session.refresh_token = EncryptedRefreshToken::new("ref", &key).unwrap();
            session.auth_scopes = AuthScopes::new(["baz", "qux"]);
            session.save(tx).await.expect("failed to update");

            // Load the updated session from the database
            let db_session = CoreSession::find_first(
                "WHERE account_id = ?",
                params![original_session.account_id.clone()],
                tx,
            )
            .await
            .unwrap()
            .unwrap();

            // This data has changed
            assert_eq!(db_session.access_token, session.access_token);
            assert_eq!(db_session.refresh_token, session.refresh_token);
            assert_eq!(db_session.auth_scopes, session.auth_scopes);

            // This data is unchanged
            assert_eq!(db_session.remote_id, original_session.remote_id);
            assert_eq!(db_session.account_id, original_session.account_id);
            assert_eq!(db_session.key_secret, original_session.key_secret);
            Ok(())
        })
        .await
        .expect("failed");
}

#[tokio::test]
async fn test_session_delete_user_id() {
    let key = SessionEncryptionKey::random();
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let account = new_test_account(&mut tether).await.unwrap();
    let session_id = SessionId::from("remote_id");
    let tokens = new_test_tokens();

    let mut session = CoreSession::new(account.remote_id, session_id, &tokens, &key).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            session.save(tx).await.expect("failed to store session");

            tx.execute(
                "DELETE FROM core_sessions WHERE account_id =?",
                params![session.account_id.clone()],
            )
            .await
            .expect("expect failed to delete user");

            let results = CoreSession::find(
                "WHERE account_id = ?",
                params![session.account_id.clone()],
                tx,
            )
            .await
            .unwrap();
            assert_eq!(results.len(), 0);
            Ok(())
        })
        .await
        .expect("failed");
}

#[tokio::test]
async fn test_session_delete_session_id() {
    let key = SessionEncryptionKey::random();
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let account = new_test_account(&mut tether).await.unwrap();
    let session_id = SessionId::from("remote_id");
    let tokens = new_test_tokens();

    let mut session = CoreSession::new(account.remote_id, session_id, &tokens, &key).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            session.save(tx).await.expect("failed to store session");

            tx.execute(
                "DELETE FROM core_sessions WHERE remote_id =?",
                params![session.remote_id.clone()],
            )
            .await
            .expect("expect failed to delete user");

            let results = CoreSession::find(
                "WHERE account_id = ?",
                params![session.account_id.clone()],
                tx,
            )
            .await
            .unwrap();
            assert_eq!(results.len(), 0);
            Ok(())
        })
        .await
        .expect("failed");
}

#[tokio::test]
async fn multiple_sessions_per_account_is_an_error() {
    let key = SessionEncryptionKey::random();
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let account = new_test_account(&mut tether).await.unwrap();
    let session_id = SessionId::from("remote_id");
    let session_id2 = SessionId::from("remote_id2");
    let tokens = new_test_tokens();

    let mut session1 =
        CoreSession::new(account.remote_id.clone(), session_id, &tokens, &key).unwrap();
    let mut session2 = CoreSession::new(account.remote_id, session_id2, &tokens, &key).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            session1.save(tx).await.expect("failed to store session");
            session2.save(tx).await.expect_err("Should fail to store.");
            Ok(())
        })
        .await
        .expect("failed");
}

#[tokio::test]
#[allow(clippy::match_wildcard_for_single_variants)] // We only care about one variant per check.
async fn test_session_observer() {
    let key = SessionEncryptionKey::random();
    let stash = new_test_connection().await;
    let mut tether = stash.connection().await.unwrap();

    let user_id1 = UserId::from("user-1");
    let user_id2 = UserId::from("user-2");

    tether
        .tx(async |tx| {
            CoreAccount::new(user_id1.clone(), String::from("name_or_addr"))
                .save(tx)
                .await?;
            CoreAccount::new(user_id2.clone(), String::from("name_or_addr"))
                .save(tx)
                .await
        })
        .await
        .unwrap();

    let mut observer = CoreSessionObserver::new(stash.clone()).await.unwrap();

    let session_id1 = SessionId::from("remote_id");
    let session_id2 = SessionId::from("remote_id2");
    let tokens = new_test_tokens();

    let mut session =
        CoreSession::new(user_id1.clone(), session_id1.clone(), &tokens, &key).unwrap();

    tether
        .tx::<_, _, StashError>(async |tx| {
            session.save(tx).await.expect("failed to store session");
            Ok(())
        })
        .await
        .expect("failed");

    let notifications = tokio::time::timeout(Duration::from_secs(1), observer.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(notifications.len(), 1);
    match &notifications[0] {
        CoreSessionObserverNotification::Created(new_session_id, new_user_id) => {
            assert_eq!(*new_session_id, session_id1);
            assert_eq!(*new_user_id, user_id1);
        }
        _ => panic!("unexpected value"),
    }

    // Create another session
    let mut session =
        CoreSession::new(user_id2.clone(), session_id2.clone(), &tokens, &key).unwrap();
    tether
        .tx::<_, _, StashError>(async |tx| {
            session.save(tx).await.expect("failed to store session");
            Ok(())
        })
        .await
        .expect("failed");

    let notifications = tokio::time::timeout(Duration::from_secs(1), observer.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(notifications.len(), 1);
    match &notifications[0] {
        CoreSessionObserverNotification::Created(new_session_id, new_user_id) => {
            assert_eq!(*new_session_id, session_id2);
            assert_eq!(*new_user_id, user_id2);
        }
        _ => panic!("unexpected value"),
    }

    // Remove a session
    tether
        .tx(async |tx| CoreSession::delete_by_id(session_id2.clone(), tx).await)
        .await
        .unwrap();

    let notifications = tokio::time::timeout(Duration::from_secs(1), observer.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(notifications.len(), 1);
    match &notifications[0] {
        CoreSessionObserverNotification::Deleted(new_session_id, new_user_id) => {
            assert_eq!(*new_session_id, session_id2);
            assert_eq!(*new_user_id, user_id2);
        }
        _ => panic!("unexpected value"),
    }

    // Remove the other session
    tether
        .tx(async |tx| CoreSession::delete_by_id(session_id1.clone(), tx).await)
        .await
        .unwrap();

    let notifications = tokio::time::timeout(Duration::from_secs(1), observer.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(notifications.len(), 1);
    match &notifications[0] {
        CoreSessionObserverNotification::Deleted(new_session_id, new_user_id) => {
            assert_eq!(*new_session_id, session_id1);
            assert_eq!(*new_user_id, user_id1);
        }
        _ => panic!("unexpected value"),
    }
}
