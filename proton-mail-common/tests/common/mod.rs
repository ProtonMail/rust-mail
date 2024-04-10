#![allow(unused)]

pub mod conversations;
pub mod init;

use proton_api_mail::proton_api_core::auth::{AccessToken, AuthScope, RefreshToken};
use proton_api_mail::proton_api_core::domain::{SecretString, Uid, UserId};
use proton_api_mail::proton_api_core::http::{APIEnvConfig, ClientBuilder};
use proton_async::runtime::MTRuntime;
use proton_core_common::db::proton_sqlite3::{SqliteConnectionPool, SqliteMode};
use proton_core_common::db::{
    DecryptedUserSession, EncryptedUserSession, SessionEncryptionKey, SessionSqliteConnection,
};
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_mail_common::{MailContext, MailUserContext};
use std::sync::Arc;
use tempdir::TempDir;
use wiremock::MockServer;

/// Test context for mail tests.
///
/// This struct provides a test context with a handcrafted new session, so that
/// we can bypass authentication. It also spins up a mock server.
///
pub struct TestContext {
    context: MailContext,
    mock_server: MockServer,
    _tmp_dir: TempDir,
    encrypted_user_session: EncryptedUserSession,
}

impl TestContext {
    /// Generate a test UID.
    fn test_uid() -> Uid {
        Uid::from("TEST_UID")
    }

    /// Create and initialize test context.
    pub fn new() -> Self {
        let runtime = MTRuntime::new(2).expect("failed to create runtime");
        let mock_server = runtime.block_on(async { MockServer::start().await });

        // Create client with the mock server as the base URL
        let mut api_env_config = APIEnvConfig::default();
        api_env_config.base_url = format!("{}/api", mock_server.uri());
        api_env_config.allow_http = true;
        api_env_config.skip_srp_proof_validation = true;
        let client = ClientBuilder::new()
            .api_env_config(api_env_config)
            .build()
            .unwrap();

        // Create a temporary directory for the database
        let tmp_dir = TempDir::new("pmc_test").expect("failed to create temp dir");
        let keychain = Arc::new(InMemoryKeyChain::default());

        // Generate a random encryption key and store it in the keychain
        let encryption_key = SessionEncryptionKey::random();
        keychain
            .store(encryption_key.to_base64())
            .expect("failed to store in keychain");

        // Create mail context
        let context = MailContext::new(
            runtime,
            tmp_dir.path(),
            tmp_dir.path(),
            keychain,
            client,
            None,
        )
        .expect("failed to create mail context");

        // Generate a fake session and write it to the database
        let pool =
            SqliteConnectionPool::new(SqliteMode::File(tmp_dir.path().join("session.db")), false);
        let mut conn =
            SessionSqliteConnection::from(pool.acquire().expect("failed to acquire connection"));

        // Create a fake session
        let session = DecryptedUserSession {
            session_id: Self::test_uid(),
            user_id: UserId::from("TEST_USER_ID"),
            name: None,
            email: "test@foo.bar".to_string(),
            refresh_token: RefreshToken(SecretString::new("REFRESHTOKEN".to_string())),
            access_token: AccessToken(SecretString::new("ACCESSTOKEN".to_string())),
            scopes: AuthScope(String::new()),
        }
        .to_encrypted_session(&encryption_key)
        .expect("failed to generate encrypted session");
        conn.tx(|tx| tx.create_or_update_session(&session))
            .expect("failed to make changes to session db");

        Self {
            mock_server,
            context,
            _tmp_dir: tmp_dir,
            encrypted_user_session: session,
        }
    }

    /// Get the mail context.
    pub fn context(&self) -> &MailContext {
        &self.context
    }

    /// Get the Wiremock server.
    pub fn mock_server(&self) -> &MockServer {
        &self.mock_server
    }

    /// Get the test user mail context.
    pub fn user_context(&self) -> MailUserContext {
        self.context
            .user_context_from_session(&self.encrypted_user_session, None)
            .expect("failed to create user context")
    }

    /// Get the async runtime.
    pub fn async_runtime(&self) -> &MTRuntime {
        self.context.async_runtime()
    }
}
