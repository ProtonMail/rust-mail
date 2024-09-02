#![allow(unused)]

pub mod account;
pub mod attachment;
pub mod conversations;
pub mod init;
mod labels;
mod messages;

use self::account::{testdata_user_secret, TEST_USER_ID, TEST_USER_MAIL};
use crate::common::account::{TEST_USER_KEY_ID, TEST_USER_PASSWORD};
use proton_api_core::auth::{SecretString, UserKeySecret};
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::Config;
use proton_core_common::datatypes::{AuthScopes, RemoteId};
use proton_core_common::db::session::{
    DecryptedUserSession, EncryptedUserSession, SessionEncryptionKey,
};
use proton_core_common::os::{InMemoryKeyChain, KeyChain};
use proton_crypto_account::keys::{ArmoredPrivateKey, KeyId, LockedKey};
use proton_crypto_account::proton_crypto::new_srp_provider;
use proton_crypto_account::salts::{KeySalt, Salt, Salts};
use proton_mail_common::{MailContext, MailUserContext};
pub use secrecy::{ExposeSecret, SecretString as RealSecretString};
use stash::orm::Model;
use stash::stash::Stash;
use std::iter;
use std::str::FromStr;
use std::sync::Arc;
use std::thread::Scope;
use tempdir::TempDir;
use url::Url;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request};

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
    fn test_uid() -> RemoteId {
        RemoteId::from("TEST_UID")
    }

    /// Create and initialize test context.
    pub async fn new() -> Self {
        Self::_new(None, None).await
    }

    /// Create and initialize test context and override the default `user_key_secret` and `user_id`.
    pub async fn with_user_secret_and_user_id(
        user_key_secret: UserKeySecret,
        user_id: RemoteId,
    ) -> Self {
        Self::_new(Some(user_key_secret), Some(user_id)).await
    }

    async fn _new(user_key_secret: Option<UserKeySecret>, user_id: Option<RemoteId>) -> Self {
        let mock_server = MockServer::start().await;

        // Create client with the mock server as the base URL
        let mut api_config = Config {
            base_url: format!("{}/api/", mock_server.uri()),
            allow_http: true,
            skip_srp_proof_validation: true,
            ..Default::default()
        };
        _ = Url::parse(&api_config.base_url).expect("Invalid URL");

        // Create a temporary directory for the database
        let tmp_dir = TempDir::new("pmc_test").expect("failed to create temp dir");
        let keychain = Arc::new(InMemoryKeyChain::default());

        let cache_path = tmp_dir.path().join("mail-cache");
        std::fs::create_dir_all(&cache_path).expect("failed to create mail cache dir");

        // Generate a random encryption key and store it in the keychain
        let encryption_key = SessionEncryptionKey::random();
        keychain
            .store(encryption_key.to_base64())
            .expect("failed to store in keychain");

        // Create mail context
        let context = MailContext::new(
            tmp_dir.path(),
            tmp_dir.path(),
            cache_path,
            100_000, // ~100kB
            keychain,
            api_config,
            None,
        )
        .await
        .expect("failed to create mail context");

        // Generate a fake session and write it to the database
        let stash =
            Stash::new(Some(&tmp_dir.path().join("session.db"))).expect("failed to create stash");

        // Create a fake session
        let mut session = DecryptedUserSession {
            session_id: Self::test_uid(),
            user_id: user_id.unwrap_or(RemoteId::from(TEST_USER_ID)),
            name_or_addr: TEST_USER_MAIL.to_owned(),
            refresh_token: RealSecretString::from_str("REFRESHTOKEN").unwrap(),
            access_token: RealSecretString::from_str("ACCESSTOKEN").unwrap(),
            key_secret: Some(user_key_secret.unwrap_or(testdata_user_secret())),
            scopes: AuthScopes::new(vec![]),
        }
        .to_encrypted_session(&encryption_key)
        .expect("failed to generate encrypted session");
        session
            .save_using(&stash.connection())
            .await
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
    pub async fn user_context(&self) -> Arc<MailUserContext> {
        self.context
            .user_context_from_session(&self.encrypted_user_session)
            .await
            .expect("failed to create user context")
    }
}
