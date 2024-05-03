#![allow(unused)]

pub mod account;
pub mod attachment;
pub mod conversations;
pub mod init;

use proton_api_mail::exports::crypto::salts::KeySecret;
use proton_api_mail::proton_api_core::auth::{AccessToken, RefreshToken, Scope, UserKeySecret};
use proton_api_mail::proton_api_core::domain::{SecretString, Uid, UserId};
use proton_api_mail::proton_api_core::http::{APIEnvConfig, Builder};
use proton_async::runtime::MultiThreaded;
use proton_core_common::db::proton_sqlite3::{SqliteConnectionPool, SqliteMode};
use proton_core_common::db::{
    DecryptedUserSession, EncryptedUserSession, SessionEncryptionKey, SessionSqliteConnection,
};
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_mail_common::{MailContext, MailUserContext};
use std::sync::Arc;
use tempdir::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request};

use self::account::{test_user_secret, TEST_USER_ID, TEST_USER_MAIL};

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
        let runtime = MultiThreaded::new(2).expect("failed to create runtime");
        let mock_server = runtime.block_on(async { MockServer::start().await });

        // Create client with the mock server as the base URL
        let mut api_env_config = APIEnvConfig {
            base_url: format!("{}/api", mock_server.uri()),
            allow_http: true,
            skip_srp_proof_validation: true,
            ..Default::default()
        };
        let client = Builder::new()
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
            user_id: UserId::from(TEST_USER_ID),
            name: None,
            email: TEST_USER_MAIL.to_owned(),
            refresh_token: RefreshToken(SecretString::new("REFRESHTOKEN".to_string())),
            access_token: AccessToken(SecretString::new("ACCESSTOKEN".to_string())),
            key_secret: Some(test_user_secret()),
            scopes: Scope(String::new()),
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

    /// Set up a catch-all mock for the mock server.
    ///
    /// Calls to this function need to come at the END of the test setup, AFTER
    /// all other mocks have been set up. This will ensure that any unconfigured
    /// calls will cause the test to fail.
    ///
    /// It is unfortunately not possible to use the [`Mock::with_priority()`]
    /// method to set this up by default as a lower-priority expectation and
    /// establish a catch-all in that way.
    ///
    pub async fn catch_all(&self) {
        // If there are any unconfigured calls, we will panic because it's not what
        // we expect to happen, so the test should fail
        Mock::given(any())
            .respond_with(|request: &Request| {
                panic!(
                    "Received unexpected {} request\n  Path: {}\n  Headers:\n{}\n  Body: {}\n",
                    request.method,
                    request.url.path(),
                    request
                        .headers
                        .iter()
                        .map(|header| format!("    {}: {:?}", header.0, header.1))
                        .collect::<Vec<String>>()
                        .join("\n"),
                    String::from_utf8(request.body.clone()).unwrap(),
                );
            })
            .mount(&self.mock_server)
            .await;
    }

    /// Get the test user mail context.
    pub fn user_context(&self) -> MailUserContext {
        self.context
            .user_context_from_session(&self.encrypted_user_session, None)
            .expect("failed to create user context")
    }

    /// Get the async runtime.
    pub fn async_runtime(&self) -> &MultiThreaded {
        self.context.async_runtime()
    }
}
