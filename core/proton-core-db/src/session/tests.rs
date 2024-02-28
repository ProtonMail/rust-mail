use crate::{DBResult, SessionEncryptionKey};

#[test]
fn test_encryption() {
    let key = SessionEncryptionKey::random();
    let ciphertext = key.encrypt(b"plaintext message".as_ref()).unwrap();
    let plaintext = key.decrypt(&ciphertext).unwrap();
    assert_eq!(&plaintext, b"plaintext message");
}

#[cfg(test)]
fn new_test_connection() -> crate::SessionSqliteConnection {
    use crate::migrations::migrate_session_db;
    use proton_sqlite3::{SqliteConnectionPool, SqliteMode};
    let pool = SqliteConnectionPool::new(SqliteMode::InMemory, false);
    let mut conn = pool.acquire().expect("failed to acquire connection");
    migrate_session_db(&mut conn).expect("failed to migrate");
    conn.into()
}

#[test]
fn test_session_store_load() {
    use crate::session::types::{DecryptedUserSession, SessionEncryptionKey, SessionId};
    use proton_api_core::domain::{ExposeSecret, SecretString, UserId};
    let session = DecryptedUserSession {
        session_id: SessionId::from("session_id"),
        user_id: UserId::from("user_id"),
        name: Some("foobar".to_string()),
        email: "foo@bar.com".to_string(),
        refresh_token: SecretString::new("token".to_string()),
        access_token: SecretString::new("access".to_string()),
        scopes: Some("Scope".to_string()),
        product: Some("Product".to_string()),
    };

    let key = SessionEncryptionKey::random();
    let encrypted_session = session
        .to_encrypted_session(&key)
        .expect("failed to encrypt");
    let mut conn = new_test_connection();
    conn.tx(|tx| -> DBResult<()> {
        tx.store_session(&encrypted_session)
            .expect("failed to store session");

        let db_encrypted_session = tx.load_session(&session.user_id).unwrap().unwrap();
        assert_eq!(encrypted_session, db_encrypted_session);
        let db_session = db_encrypted_session.to_decrypted_session(&key).unwrap();
        assert_eq!(db_session.session_id, session.session_id);
        assert_eq!(db_session.user_id, session.user_id);
        assert_eq!(db_session.name, session.name);
        assert_eq!(db_session.email, session.email);
        assert_eq!(db_session.scopes, session.scopes);
        assert_eq!(db_session.product, session.product);
        assert_eq!(
            db_session.access_token.expose_secret(),
            session.access_token.expose_secret()
        );
        assert_eq!(
            db_session.refresh_token.expose_secret(),
            session.refresh_token.expose_secret()
        );
        Ok(())
    })
    .expect("failed");
}
