use crate::db::{migrate_session_db, SessionSqliteConnection};
use crate::db::{DBResult, SessionEncryptionKey};
use proton_api_core::auth::UserKeySecret;
use proton_api_core::auth::{AccessToken, RefreshToken, Scope};

#[cfg(test)]
fn new_test_connection() -> SessionSqliteConnection {
    use proton_sqlite3::{SqliteConnectionPool, SqliteMode};
    let pool = SqliteConnectionPool::new(SqliteMode::InMemory, false);
    let mut conn = pool.acquire().expect("failed to acquire connection");
    migrate_session_db(&mut conn).expect("failed to migrate");
    conn.into()
}

#[test]
fn test_encryption() {
    let key = SessionEncryptionKey::random();
    let ciphertext = key.encrypt(b"plaintext message".as_ref()).unwrap();
    let plaintext = key.decrypt(&ciphertext).unwrap();
    assert_eq!(&plaintext, b"plaintext message");
}

#[test]
fn test_session_store_load() {
    use crate::db::session::types::{DecryptedUserSession, SessionEncryptionKey};
    use proton_api_core::domain::{Uid, UserId};
    let session = DecryptedUserSession {
        session_id: Uid::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        key_secret: Some(UserKeySecret::from(vec![1, 2, 3, 4])),
        scopes: Scope::from("Scope"),
    };

    let key = SessionEncryptionKey::random();
    let encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");
    let mut conn = new_test_connection();
    conn.tx(|tx| -> DBResult<()> {
        tx.create_or_update_session(&encrypted_session)
            .expect("failed to store session");

        let db_encrypted_session = tx
            .get_session_with_user_id(&session.user_id)
            .unwrap()
            .unwrap();
        assert_eq!(encrypted_session, db_encrypted_session);
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
        assert_eq!(
            db_session
                .key_secret
                .as_ref()
                .expect("key secret must be there")
                .expose_secret()
                .as_bytes(),
            session
                .key_secret
                .as_ref()
                .expect("key secret must be there")
                .expose_secret()
                .as_bytes()
        );
        Ok(())
    })
    .expect("failed");
}

#[test]
fn test_session_update() {
    use crate::db::session::types::{DecryptedUserSession, SessionEncryptionKey};
    use proton_api_core::domain::{Uid, UserId};
    let session = DecryptedUserSession {
        session_id: Uid::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        key_secret: Some(UserKeySecret::from(vec![1, 2, 3, 4])),
        scopes: Scope::from("Scope"),
    };

    let updated_session = DecryptedUserSession {
        session_id: Uid::from("session_id_2"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("refreshed".to_string()),
        access_token: AccessToken::from("another token".to_string()),
        key_secret: Some(UserKeySecret::from(vec![1, 2, 3, 4])),
        scopes: Scope::from("Scope Scope2"),
    };

    let key = SessionEncryptionKey::random();
    let encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");
    let encrypted_updated_session = updated_session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");

    let mut conn = new_test_connection();
    conn.tx(|tx| -> DBResult<()> {
        tx.create_or_update_session(&encrypted_session)
            .expect("failed to store session");

        tx.update_session(
            &updated_session.user_id,
            &updated_session.session_id,
            &encrypted_updated_session.access_token,
            &encrypted_updated_session.refresh_token,
            &updated_session.scopes,
        )
        .expect("failed to update");
        let db_encrypted_session = tx
            .get_session_with_user_id(&session.user_id)
            .unwrap()
            .unwrap();
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
        db_session
            .key_secret
            .expect("Key secret should still be there after update");
        Ok(())
    })
    .expect("failed");
}

#[test]
fn test_session_delete_user_id() {
    use crate::db::session::types::{DecryptedUserSession, SessionEncryptionKey};
    use proton_api_core::domain::{Uid, UserId};
    let session = DecryptedUserSession {
        session_id: Uid::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        key_secret: Some(UserKeySecret::from(vec![1, 2, 3, 4])),
        scopes: Scope::from("Scope"),
    };
    let key = SessionEncryptionKey::random();
    let encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");

    let mut conn = new_test_connection();
    conn.tx(|tx| -> DBResult<()> {
        tx.create_or_update_session(&encrypted_session)
            .expect("failed to store session");
        tx.delete_session_with_user_id(&session.user_id)
            .expect("expect failed to delete user");

        let db_encrypted_session = tx.get_session_with_user_id(&session.user_id).unwrap();
        assert!(db_encrypted_session.is_none());
        Ok(())
    })
    .expect("failed");
}

#[test]
fn test_session_delete_session_id() {
    use crate::db::session::types::{DecryptedUserSession, SessionEncryptionKey};
    use proton_api_core::domain::{Uid, UserId};
    let session = DecryptedUserSession {
        session_id: Uid::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: RefreshToken::from("token".to_string()),
        access_token: AccessToken::from("access".to_string()),
        key_secret: Some(UserKeySecret::from(vec![1, 2, 3, 4])),
        scopes: Scope::from("Scope"),
    };
    let key = SessionEncryptionKey::random();
    let encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");

    let mut conn = new_test_connection();
    conn.tx(|tx| -> DBResult<()> {
        tx.create_or_update_session(&encrypted_session)
            .expect("failed to store session");
        tx.delete_session(&session.session_id)
            .expect("expect failed to delete user");

        let db_encrypted_session = tx.get_session_with_user_id(&session.user_id).unwrap();
        assert!(db_encrypted_session.is_none());
        Ok(())
    })
    .expect("failed");
}
