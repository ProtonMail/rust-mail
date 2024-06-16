use std::sync::Arc;
use futures::executor::block_on;
use std::io::stdout;
use tracing::subscriber::set_global_default;
use tracing::Level;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{registry, EnvFilter};

use account::{testdata_user_secret, TEST_USER_ID, TEST_USER_MAIL};
use proton_api_core::{
    auth::{AccessToken, RefreshToken, Scope, UserKeySecret},
    domain::{
        Address, ContactEmailEvent, ContactEvent, Event, EventId, ProductUsedSpace, SecretString,
        Uid, User, UserId, UserSettings,
    },
    exports::serde::{self, Deserialize, Serialize},
    http::{APIEnvConfig, Builder},
};
use proton_core_common::{
    db::{
        DecryptedUserSession, EncryptedUserSession,
        SessionEncryptionKey,
    },
    os::{InMemoryKeyChain, KeyChain},
    Context, CoreEvent, CoreEventSubscriberConnectionProvider, UserContext,
    UserDatabaseInitializer,
};
use tempdir::TempDir;
use wiremock::{matchers::any, Mock, MockServer, Request};
use proton_sqlite3::MigratorError;
use stash::orm::Model;
use stash::stash::Stash;

pub mod account;
pub mod contacts;

struct TestCoreDatabaseInitializer {}

impl UserDatabaseInitializer for TestCoreDatabaseInitializer {
    fn initialize(&self, _stash: &Stash) -> Result<(), MigratorError> {
        Ok(())
    }
}

/// Test context for testing the core context.
///
/// This struct provides a test context with a handcrafted new session, so that
/// we can bypass authentication. It also spins up a mock server.
///
pub struct TestContext {
    context: Context,
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
    pub async fn new() -> Self {
        drop(set_global_default(
            registry()
                .with(EnvFilter::new(
                    "debug,stash=debug",
                ))
                .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
        ));
        let user_key_secret: Option<UserKeySecret> = None;
        let user_id: Option<UserId> = None;
        let mock_server = MockServer::start().await;

        // Create client with the mock server as the base URL
        let api_env_config = APIEnvConfig {
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
        let tmp_dir = TempDir::new("account_test").expect("failed to create temp dir");
        let keychain = Arc::new(InMemoryKeyChain::default());

        let cache_path = tmp_dir.path().join("core-cache");
        std::fs::create_dir_all(cache_path).expect("failed to create mail cache dir");

        // Generate a random encryption key and store it in the keychain
        let encryption_key = SessionEncryptionKey::random();
        keychain
            .store(encryption_key.to_base64())
            .expect("failed to store in keychain");

        // Create a core context
        let initializers: Vec<Box<dyn UserDatabaseInitializer>> =
            vec![Box::new(TestCoreDatabaseInitializer {})];
        let core_context = Context::new(
            tmp_dir.path(),
            tmp_dir.path(),
            keychain,
            initializers,
            client,
            None,
        ).await
        .expect("failed to create context");

        // Generate a fake session and write it to the database
        let path = tmp_dir.path().join("session.db");
        let stash = Stash::new(Some(&path)).expect("failed to create stash");

        // Create a fake session
        let mut session = DecryptedUserSession {
            session_id: Self::test_uid(),
            user_id: user_id.unwrap_or(UserId::from(TEST_USER_ID)),
            name: None,
            email: TEST_USER_MAIL.to_owned(),
            refresh_token: RefreshToken(SecretString::new("REFRESHTOKEN".to_string())),
            access_token: AccessToken(SecretString::new("ACCESSTOKEN".to_string())),
            key_secret: Some(user_key_secret.unwrap_or(testdata_user_secret())),
            scopes: Scope(String::new()),
        }
        .to_encrypted_session(&encryption_key)
        .expect("failed to generate encrypted session");
        session.set_stash(&stash);
        session.save().await
            .expect("failed to make changes to session db");

        Self {
            mock_server,
            context: core_context,
            _tmp_dir: tmp_dir,
            encrypted_user_session: session,
        }
    }

    /// Get the mail context.
    #[allow(dead_code)]
    pub fn context(&self) -> &Context {
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

    /// Get the test user context.
    pub async fn user_context(&self) -> UserContext {
        self.context
            .user_context_from_session(&self.encrypted_user_session, None).await
            .expect("failed to create user context")
    }
}

impl CoreEventSubscriberConnectionProvider for &TestContext {
    fn get_user_id_and_db_connection(
        &self,
    ) -> proton_api_core::exports::anyhow::Result<(UserId, Stash)> {
        let user_ctx = block_on(async { self.user_context().await });
        Ok((
            user_ctx.user_id().clone(),
            user_ctx.stash().clone(),
        ))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(crate = "self::serde")]
pub struct TestCoreEvent {
    pub event_id: EventId,
    pub user: Option<User>,
    pub user_settings: Option<UserSettings>,
    pub address: Option<Vec<Address>>,
    pub contacts: Option<Vec<ContactEvent>>,
    pub contact_emails: Option<Vec<ContactEmailEvent>>,
}

impl Event for TestCoreEvent {
    fn event_id(&self) -> &EventId {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        false
    }
}

impl CoreEvent for TestCoreEvent {
    fn get_core_event_user(&self) -> Option<&User> {
        self.user.as_ref()
    }
    fn get_core_event_user_mut(&mut self) -> Option<&mut User> {
        self.user.as_mut()
    }

    fn get_core_event_user_settings(&self) -> Option<&UserSettings> {
        self.user_settings.as_ref()
    }
    fn get_core_event_user_settings_mut(&mut self) -> Option<&mut UserSettings> {
        self.user_settings.as_mut()
    }

    fn get_core_event_addresses(&self) -> Option<&[Address]> {
        self.address.as_deref()
    }
    fn get_core_event_addresses_mut(&mut self) -> Option<&mut [Address]> {
        self.address.as_mut().map(|vec| vec.as_mut_slice())
    }

    fn get_core_event_used_space(&self) -> Option<i64> {
        None
    }

    fn get_core_event_used_product_space(&self) -> Option<&ProductUsedSpace> {
        None
    }

    fn get_core_event_contacts(&self) -> Option<&[ContactEvent]> {
        self.contacts.as_deref()
    }
    fn get_core_event_contacts_mut(&mut self) -> Option<&mut [ContactEvent]> {
        self.contacts.as_mut().map(|vec| vec.as_mut_slice())
    }

    fn get_core_event_contact_emails(&self) -> Option<&[ContactEmailEvent]> {
        self.contact_emails.as_deref()
    }
    fn get_core_event_contact_emails_mut(&mut self) -> Option<&mut [ContactEmailEvent]> {
        self.contact_emails.as_mut().map(|vec| vec.as_mut_slice())
    }
}

impl Default for TestCoreEvent {
    fn default() -> Self {
        Self {
            event_id: EventId::from("test_event"),
            user: Option::default(),
            user_settings: Option::default(),
            address: Option::default(),
            contacts: Option::default(),
            contact_emails: Option::default(),
        }
    }
}
