use crate::db::{EncryptedUserSession, SessionEncryptionKey};
use proton_api_core::auth::{AccessToken, RefreshToken, Scope};
use stash::orm::Model;
use stash::params;
use stash::stash::Stash;

#[test]
fn test_encryption() {
    let key = SessionEncryptionKey::random();
    let ciphertext = key.encrypt(b"plaintext message".as_ref()).unwrap();
    let plaintext = key.decrypt(&ciphertext).unwrap();
    assert_eq!(&plaintext, b"plaintext message");
}

#[cfg(test)]
async fn new_test_connection() -> Stash {
    use crate::db::migrations::migrate_session_db;
    let stash = Stash::new(None).expect("Failed to create Stash");
    migrate_session_db(&stash).await.expect("failed to migrate");
    stash
}

#[tokio::test]
async fn test_session_store_load() {
    use crate::db::session::types::{DecryptedUserSession, SessionEncryptionKey};
    use proton_api_core::domain::{Uid, UserId};
    let session = DecryptedUserSession {
        session_id: Uid::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        scopes: Scope::from("Scope"),
    };

    let key = SessionEncryptionKey::random();
    let mut encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");
    let stash = new_test_connection().await;
    {
        let tx = stash.transaction().await.expect("failed to start transaction");
        encrypted_session.save_using(&tx).await.expect("failed to store session");
        encrypted_session.set_stash(&stash);

        let results = tx
            .query::<_, EncryptedUserSession>("SELECT rowid AS rowid, * FROM core_sessions WHERE user_id=?".to_owned(), params![session.user_id.clone()]).await.unwrap();
        let db_encrypted_session = results.first().unwrap();
        assert_eq!(encrypted_session, *db_encrypted_session);
        let db_session = db_encrypted_session.to_decrypted_session(&key).unwrap();
        assert_eq!(db_session.session_id, session.session_id);
        assert_eq!(db_session.user_id, session.user_id);
        assert_eq!(db_session.name, session.name);
        assert_eq!(db_session.email, session.email);
        assert_eq!(db_session.scopes, session.scopes);
        assert_eq!(
            db_session.access_token.expose_secret(),
            session.access_token.expose_secret()
        );
        assert_eq!(
            db_session.refresh_token.expose_secret(),
            session.refresh_token.expose_secret()
        );
        tx.commit().await
    }
    .expect("failed");
}

#[tokio::test]
async fn test_session_update() {
    use crate::db::session::types::{DecryptedUserSession, SessionEncryptionKey};
    use proton_api_core::domain::{Uid, UserId};
    let session = DecryptedUserSession {
        session_id: Uid::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        scopes: Scope::from("Scope"),
    };

    let updated_session = DecryptedUserSession {
        session_id: Uid::from("session_id_2"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        scopes: Scope::from("Scope Scope2"),
    };

    let key = SessionEncryptionKey::random();
    let mut encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");
    
    let stash = new_test_connection().await;
    {
        let tx = stash.transaction().await.expect("failed to start transaction");
        encrypted_session.save_using(&tx).await.expect("failed to store session");
        encrypted_session.session_id = updated_session.session_id.clone();
        encrypted_session.scopes = updated_session.scopes.clone();
        encrypted_session.save_using(&tx).await.expect("failed to update");
        encrypted_session.set_stash(&stash);
        let results = tx
            .query::<_, EncryptedUserSession>("SELECT rowid AS rowid, * FROM core_sessions WHERE user_id=?".to_owned(), params![session.user_id.clone()]).await.unwrap();
        let db_encrypted_session = results.first().unwrap();
        assert_eq!(encrypted_session, *db_encrypted_session);
        let db_session = db_encrypted_session.to_decrypted_session(&key).unwrap();
        assert_eq!(db_session.session_id, updated_session.session_id);
        assert_eq!(db_session.user_id, updated_session.user_id);
        assert_eq!(db_session.name, updated_session.name);
        assert_eq!(db_session.email, updated_session.email);
        assert_eq!(db_session.scopes, updated_session.scopes);
        assert_eq!(
            db_session.access_token.expose_secret(),
            updated_session.access_token.expose_secret()
        );
        assert_eq!(
            db_session.refresh_token.expose_secret(),
            updated_session.refresh_token.expose_secret()
        );
        tx.commit().await
    }
    .expect("failed");
}

#[tokio::test]
async fn test_session_delete_user_id() {
    use crate::db::session::types::{DecryptedUserSession, SessionEncryptionKey};
    use proton_api_core::domain::{Uid, UserId};
    let session = DecryptedUserSession {
        session_id: Uid::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        scopes: Scope::from("Scope"),
    };
    let key = SessionEncryptionKey::random();
    let mut encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");
    
    let stash = new_test_connection().await;
    {
        let tx = stash.transaction().await.expect("failed to start transaction");
        encrypted_session.save_using(&tx).await.expect("failed to store session");
        encrypted_session.set_stash(&stash);
        tx.execute("DELETE FROM core_sessions WHERE user_id =?", params![session.user_id.clone()]).await.expect("expect failed to delete user");

        let results = tx.query::<_, EncryptedUserSession>("SELECT rowid AS rowid, * FROM core_sessions WHERE user_id=?".to_owned(), params![session.user_id.clone()]).await.unwrap();
        assert_eq!(results.len(), 0);
        tx.commit().await
    }
    .expect("failed");
}

#[tokio::test]
async fn test_session_delete_session_id() {
    use crate::db::session::types::{DecryptedUserSession, SessionEncryptionKey};
    use proton_api_core::domain::{Uid, UserId};
    let session = DecryptedUserSession {
        session_id: Uid::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        scopes: Scope::from("Scope"),
    };
    let key = SessionEncryptionKey::random();
    let mut encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");
    
    let stash = new_test_connection().await;
    {
        let tx = stash.transaction().await.expect("failed to start transaction");
        encrypted_session.save_using(&tx).await.expect("failed to store session");
        encrypted_session.set_stash(&stash);
        tx.execute("DELETE FROM core_sessions WHERE session_id =?", params![session.session_id.clone()]).await.expect("expect failed to delete user");
        
        let results = tx.query::<_, EncryptedUserSession>("SELECT rowid AS rowid, * FROM core_sessions WHERE user_id=?".to_owned(), params![session.user_id.clone()]).await.unwrap();
        assert_eq!(results.len(), 0);
        tx.commit().await
    }
    .expect("failed");
}
