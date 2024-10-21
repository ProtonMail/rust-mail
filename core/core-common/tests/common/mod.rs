use account::{testdata_user_secret, TEST_USER_ID, TEST_USER_MAIL};
use futures::executor::block_on;
use proton_api_core::auth::{AuthSession, AuthState};
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Action as ApiAction, Address as ApiAddress, ContactEmailEvent as ApiContactEmailEvent,
    ContactEvent as ApiContactEvent, User as ApiUser, UserSettings as ApiUserSettings,
};
use proton_api_core::services::proton::responses::GetEventResponse;
use proton_api_core::services::proton::Config as ApiConfig;
use proton_core_common::datatypes::{PasswordMode, ProductUsedSpace, RemoteId, TfaStatus};
use proton_core_common::db::account::{CoreAccount, CoreSession};
use proton_core_common::events::{Action, ContactEmailEvent, ContactEvent};
use proton_core_common::models::{Address, ModelExtension, User, UserSettings};
use proton_core_common::{
    db::account::SessionEncryptionKey,
    os::{InMemoryKeyChain, KeyChain},
    Context, CoreEvent, CoreEventSubscriberConnectionProvider, UserContext,
    UserDatabaseInitializer,
};
use proton_event_loop::Event;
use proton_sqlite3::MigratorError;
use secrecy::SecretString;
use serde::Deserialize;
use stash::stash::Stash;
use std::io::stdout;
use std::sync::{Arc, Weak};
use tempdir::TempDir;
use tracing::subscriber::set_global_default;
use tracing::Level;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{registry, EnvFilter};
use url::Url;
use wiremock::{matchers::any, Mock, MockServer, Request};

pub mod account;
pub mod contacts;
mod images_logo;

struct TestCoreDatabaseInitializer;

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
#[allow(unused)]
pub struct TestContext {
    this: Weak<Self>,
    context: Arc<Context>,
    mock_server: MockServer,
    tmp_dir: TempDir,
    core_account: CoreAccount,
    core_session: CoreSession,
}

impl TestContext {
    /// Generate a test UID.
    fn test_uid() -> RemoteId {
        RemoteId::from("TEST_UID")
    }

    /// Generate a test user ID.
    fn test_user_id() -> RemoteId {
        RemoteId::from(TEST_USER_ID)
    }

    /// Generate a test user name or address.
    fn test_user_mail() -> String {
        TEST_USER_MAIL.to_owned()
    }

    /// Generate a test access token.
    fn test_acctok() -> SecretString {
        SecretString::from("ACCESSTOKEN".to_owned())
    }

    /// Generate a test refresh token.
    fn test_reftok() -> SecretString {
        SecretString::from("REFRESHTOKEN".to_owned())
    }

    /// Generate test scopes.
    fn test_scopes() -> Vec<String> {
        vec!["foo".to_owned(), "bar".to_owned()]
    }

    /// Create and initialize test context.
    pub async fn new() -> Arc<Self> {
        drop(set_global_default(
            registry()
                .with(EnvFilter::new("debug,stash=debug"))
                .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
        ));
        let mock_server = MockServer::start().await;

        // Create client with the mock server as the base URL
        let api_env_config = ApiConfig {
            base_url: format!("{}/api/", mock_server.uri()),
            allow_http: true,
            skip_srp_proof_validation: true,
            ..Default::default()
        };
        _ = Url::parse(&api_env_config.base_url).expect("Invalid URL");

        // Create a temporary directory for the database
        let tmp_dir = TempDir::new("account_test").expect("failed to create temp dir");
        let keychain = Arc::new(InMemoryKeyChain::default());

        let cache_path = tmp_dir.path().join("core-cache");
        std::fs::create_dir_all(&cache_path).expect("failed to create mail cache dir");

        // Generate a random encryption key and store it in the keychain
        let encryption_key = SessionEncryptionKey::random();
        keychain
            .store(encryption_key.to_base64())
            .expect("failed to store in keychain");

        // Create a core context
        let context = Context::new(
            tmp_dir.path(),
            tmp_dir.path(),
            keychain,
            [TestCoreDatabaseInitializer.boxed()],
            api_env_config,
            None,
        )
        .await
        .expect("failed to create context");

        // Generate a fake session and write it to the database
        let (core_account, core_session) = {
            // Create a temporary stash just to insert the fake data.
            let path = tmp_dir.path().join("account.db");
            let stash = Stash::new(Some(&path)).expect("failed to create stash");

            // Create a fake account.
            let account = CoreAccount::new(
                Self::test_user_id(),
                Self::test_user_mail(),
                TfaStatus::None,
                PasswordMode::One,
            )
            .with_stash(&stash)
            .with_save()
            .await
            .expect("fake account should save");

            // Create a auth session.
            let auth = AuthSession {
                uid: Self::test_uid().into(),
                name_or_addr: Self::test_user_mail(),
                user_id: Self::test_user_id().into(),
                second_factor_mode: TfaStatus::None.into(),
                password_mode: PasswordMode::One.into(),
                access_token: Self::test_acctok().into(),
                refresh_token: Self::test_reftok().into(),
                auth_scope: Self::test_scopes(),
                auth_state: AuthState::Ready,
            };

            // Create a fake session.
            let session = CoreSession::new(auth, &encryption_key)
                .expect("session should be created")
                .with_key_secret(&testdata_user_secret(), &encryption_key)
                .expect("key secret should be set")
                .with_stash(&stash)
                .with_save()
                .await
                .expect("fake session should save");

            (account, session)
        };

        Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            mock_server,
            context,
            tmp_dir,
            core_account,
            core_session,
        })
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
        let cache_path = self.tmp_dir.path().join("image_cache");
        self.context
            .user_context_from_session(
                &self.core_session,
                cache_path,
                100_000, // ~100kB
            )
            .await
            .expect("failed to create user context")
    }
}

impl CoreEventSubscriberConnectionProvider for TestContext {
    fn get_user_id_and_db_connection(&self) -> anyhow::Result<(RemoteId, Stash)> {
        let user_ctx = block_on(async { self.user_context().await });
        Ok((user_ctx.user_id().clone(), user_ctx.stash().clone()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TestApiCoreEvent {
    pub event_id: ApiRemoteId,
    pub action: ApiAction,
    pub address: Option<Vec<ApiAddress>>,
    pub contact_emails: Option<Vec<ApiContactEmailEvent>>,
    pub contacts: Option<Vec<ApiContactEvent>>,
    pub has_more: bool,
    pub user: Option<ApiUser>,
    pub user_settings: Option<ApiUserSettings>,
}

impl GetEventResponse for TestApiCoreEvent {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestCoreEvent {
    pub event_id: RemoteId,
    pub action: Action,
    pub address: Option<Vec<Address>>,
    pub contact_emails: Option<Vec<ContactEmailEvent>>,
    pub contacts: Option<Vec<ContactEvent>>,
    pub has_more: bool,
    pub user: Option<User>,
    pub user_settings: Option<UserSettings>,
}

impl Event for TestCoreEvent {
    type Id = RemoteId;
    type Response = TestApiCoreEvent;

    fn event_id(&self) -> &Self::Id {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        false
    }
}

impl From<TestApiCoreEvent> for TestCoreEvent {
    fn from(value: TestApiCoreEvent) -> Self {
        Self {
            event_id: value.event_id.into(),
            action: value.action.into(),
            address: value
                .address
                .map(|vec| vec.into_iter().map(Address::from).collect()),
            contact_emails: value
                .contact_emails
                .map(|vec| vec.into_iter().map(ContactEmailEvent::from).collect()),
            contacts: value
                .contacts
                .map(|vec| vec.into_iter().map(ContactEvent::from).collect()),
            has_more: value.has_more,
            user: value.user.map(User::from),
            user_settings: value.user_settings.map(UserSettings::from),
        }
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
        self.address.as_deref_mut()
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
        self.contacts.as_deref_mut()
    }

    fn get_core_event_contact_emails(&self) -> Option<&[ContactEmailEvent]> {
        self.contact_emails.as_deref()
    }
    fn get_core_event_contact_emails_mut(&mut self) -> Option<&mut [ContactEmailEvent]> {
        self.contact_emails.as_deref_mut()
    }
}

impl Default for TestCoreEvent {
    fn default() -> Self {
        Self {
            event_id: RemoteId::from("test_event"),
            action: Action::Create,
            address: None,
            contact_emails: None,
            contacts: None,
            has_more: false,
            user: None,
            user_settings: None,
        }
    }
}
