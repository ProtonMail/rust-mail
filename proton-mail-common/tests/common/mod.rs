pub mod conversations;
pub mod init;

use proton_api_mail::proton_api_core::auth::{AccessToken, AuthScope, RefreshToken};
use proton_api_mail::proton_api_core::domain::{SecretString, Uid, UserId};
use proton_api_mail::proton_api_core::http;
use proton_api_mail::proton_api_core::http::APIEnvConfig;
use proton_async::runtime::MTRuntime;
use proton_core_common::db::proton_sqlite3::SqliteMode;
use proton_core_common::db::SessionEncryptionKey;
use proton_core_common::db::{EncryptedUserSession, SessionSqliteConnection};
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_mail_common::{MailContext, MailUserContext};
use std::sync::Arc;
use wiremock::MockServer;

/// Sets up a test context which handcrafts a new session so that we can bypass the authentication
/// as well as a mock server.
pub struct TestContext {
    context: MailContext,
    mock_server: MockServer,
    _tmp_dir: tempdir::TempDir,
    encrypted_user_session: EncryptedUserSession,
}

impl TestContext {
    fn test_uid() -> Uid {
        Uid::from("TEST_UID")
    }

    /// Create and initialize test context.
    pub fn new() -> Self {
        let runtime = MTRuntime::new(2).expect("failed to create runtime");
        let mock_server = runtime.block_on(async { MockServer::start().await });

        let mut api_env_config = APIEnvConfig::default();
        api_env_config.base_url = format!("{}/api", mock_server.uri());
        api_env_config.allow_http = true;
        api_env_config.skip_srp_proof_validation = true;
        let client = http::ClientBuilder::new()
            .api_env_config(api_env_config)
            .build()
            .unwrap();

        let tmp_dir = tempdir::TempDir::new("pmc_test").expect("failed to create temp dir");
        let keychain = Arc::new(InMemoryKeyChain::default());

        let encryption_key = SessionEncryptionKey::random();
        keychain
            .store(encryption_key.to_base64())
            .expect("failed to store in keychain");
        let context = MailContext::new(
            runtime,
            tmp_dir.path(),
            tmp_dir.path(),
            keychain,
            client,
            None,
        )
        .expect("failed to create mail context");

        // generate a fake session and write it to the database.

        let pool = proton_core_common::db::proton_sqlite3::SqliteConnectionPool::new(
            SqliteMode::File(tmp_dir.path().join("session.db")),
            false,
        );
        let mut conn =
            SessionSqliteConnection::from(pool.acquire().expect("failed to acquire connection"));

        let session = proton_core_common::db::DecryptedUserSession {
            session_id: Self::test_uid(),
            user_id: UserId::from("TEST_USER_ID"),
            name: None,
            email: "test@foo.bar".to_string(),
            refresh_token: RefreshToken(SecretString::new("REFRESHTOKEN".to_string())),
            access_token: AccessToken(SecretString::new("ACCESSTOKEN".to_string())),
            scopes: AuthScope(String::new()),
        };

        let session = session
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
    #[allow(unused)]
    pub fn context(&self) -> &MailContext {
        &self.context
    }

    /// Get the wiremock server
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
