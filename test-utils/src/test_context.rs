use crate::core::account::{testdata_user_secret, TEST_USER_ID, TEST_USER_MAIL};
use crate::helpers::Helpers;
use futures::executor::block_on;
use once_cell::sync::Lazy;
use proton_api_core::auth::{AuthSession, AuthState, SecretString, UserKeySecret};
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::response_data::{
    Action as ApiAction, Address as ApiAddress, ContactEmailEvent as ApiContactEmailEvent,
    ContactEvent as ApiContactEvent, User as ApiUser, UserSettings as ApiUserSettings,
};
use proton_api_core::services::proton::responses::GetEventResponse;
use proton_api_core::services::proton::Config as ApiConfig;
use proton_core_common::datatypes::ProductUsedSpace;
use proton_core_common::datatypes::{PasswordMode, RemoteId, TfaStatus};
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
use proton_mail_common::{MailContext, MailUserContext};
use proton_sqlite3::MigratorError;
use serde::Deserialize;
use stash::stash::Stash;
use std::io::stdout;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::Weak;
use tempdir::TempDir;
use tracing::subscriber::set_global_default;
use tracing::Level;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{registry, EnvFilter};
use url::Url;
use wiremock::MockServer;
use wiremock::{matchers::any, Mock, Request};

static HELPERS: Lazy<Mutex<Helpers>> = Lazy::new(|| Mutex::new(Helpers::new()));

pub trait BaseTestContext {
    fn helpers() -> MutexGuard<'static, Helpers> {
        HELPERS.lock().unwrap()
    }

    /// Generate a test UID.
    #[must_use]
    fn test_uid() -> RemoteId {
        RemoteId::from("TEST_UID")
    }

    /// Generate a test user ID.
    #[must_use]
    fn test_user_id() -> RemoteId {
        RemoteId::from(TEST_USER_ID)
    }

    /// Generate a test user name or address.
    #[must_use]
    fn test_user_mail() -> String {
        TEST_USER_MAIL.to_owned()
    }

    /// Generate a test access token.
    #[must_use]
    fn test_acctok() -> SecretString {
        SecretString::from("ACCESSTOKEN".to_owned())
    }

    /// Generate a test refresh token.
    #[must_use]
    fn test_reftok() -> SecretString {
        SecretString::from("REFRESHTOKEN".to_owned())
    }

    /// Generate test scopes.
    #[must_use]
    fn test_scopes() -> Vec<String> {
        vec!["foo".to_owned(), "bar".to_owned()]
    }

    #[must_use]
    fn keychain() -> Arc<InMemoryKeyChain> {
        Arc::new(InMemoryKeyChain::default())
    }

    #[must_use]
    fn encryption_key() -> SessionEncryptionKey {
        SessionEncryptionKey::random()
    }

    fn store_encryption_key_in_keychain(
        keychain: Arc<InMemoryKeyChain>,
        encryption_key: SessionEncryptionKey,
    ) {
        keychain
            .store(encryption_key.to_base64())
            .expect("failed to store in keychain");
    }

    #[must_use]
    fn api_config(mock_web_server: &MockServer) -> ApiConfig {
        let api_config = ApiConfig {
            base_url: format!("{}/api/", mock_web_server.uri()),
            allow_http: true,
            skip_srp_proof_validation: true,
            ..Default::default()
        };
        _ = Url::parse(&api_config.base_url).expect("Invalid URL");
        api_config
    }
}

struct TestCoreDatabaseInitializer;

impl UserDatabaseInitializer for TestCoreDatabaseInitializer {
    fn initialize(&self, _stash: &Stash) -> Result<(), MigratorError> {
        Ok(())
    }
}

#[allow(dead_code)]
pub struct TestContext {
    this: Weak<Self>,
    pub context: Arc<Context>,
    pub tmp_dir: Arc<TempDir>,
    pub core_account: CoreAccount,
    pub core_session: CoreSession,
    pub mock_web_server: MockServer,
    pub mail_context: MailContext,
}

impl BaseTestContext for TestContext {}

impl TestContext {
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.context
    }

    #[must_use]
    pub fn mail_context(&self) -> MailContext {
        self.mail_context.clone()
    }

    #[must_use]
    pub fn mock_server(&self) -> &MockServer {
        &self.mock_web_server
    }

    /// Create and initialize test context.
    pub async fn new() -> Arc<Self> {
        Self::_new(None, None).await
    }

    /// Create and initialize test context and override the default `user_key_secret` and `user_id`.
    pub async fn with_user_secret_and_user_id(
        user_key_secret: UserKeySecret,
        user_id: RemoteId,
    ) -> Arc<Self> {
        Self::_new(Some(user_key_secret), Some(user_id)).await
    }

    async fn _new(user_key_secret: Option<UserKeySecret>, user_id: Option<RemoteId>) -> Arc<Self> {
        drop(set_global_default(
            registry()
                .with(EnvFilter::new("debug,stash=debug"))
                .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
        ));

        let mock_web_server = MockServer::start().await;
        let tmp_dir = Self::helpers().provide_tmp_dir("core_test");
        let keychain: Arc<InMemoryKeyChain> = Self::keychain();
        let api_config: proton_api_core::services::proton::Config =
            Self::api_config(&mock_web_server);
        let encryption_key = Self::encryption_key();

        keychain
            .store(encryption_key.to_base64())
            .expect("failed to store in keychain");

        // Use the given data or fall back to the default
        let user_id = user_id.unwrap_or_else(Self::test_user_id);
        let user_key_secret = user_key_secret.unwrap_or_else(testdata_user_secret);

        // Create core test context
        let context = Context::new(
            tmp_dir.path(),
            tmp_dir.path(),
            keychain.clone(),
            [TestCoreDatabaseInitializer.boxed()],
            api_config.clone(),
            None,
        )
        .await
        .expect("failed to create mail context");

        let mail_cache_path = tmp_dir.path().join("mail-cache");
        std::fs::create_dir_all(&mail_cache_path).expect("failed to create mail cache dir");

        // Generate a fake session and write it to the database
        let (core_account, core_session) = {
            // Create a temporary stash just to insert the fake data.
            let path = tmp_dir.path().join("account.db");
            let stash = Stash::new(Some(&path)).expect("failed to create stash");

            // Create a fake account.
            let account = CoreAccount::new(
                user_id.clone(),
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
                user_id: user_id.clone().into(),
                second_factor_mode: TfaStatus::None.into(),
                password_mode: PasswordMode::One.into(),
                access_token: Self::test_acctok(),
                refresh_token: Self::test_reftok(),
                auth_scope: Self::test_scopes(),
                auth_state: AuthState::Ready,
            };

            // Create a fake session.
            let session = CoreSession::new(auth, &encryption_key)
                .expect("session should be created")
                .with_key_secret(&user_key_secret, &encryption_key)
                .expect("key secret should be set")
                .with_stash(&stash)
                .with_save()
                .await
                .expect("fake session should save");

            (account, session)
        };

        // Create mail test context
        let mail_context = MailContext::new(
            tmp_dir.path(),
            tmp_dir.path(),
            mail_cache_path,
            100_000, // ~100kB
            keychain,
            api_config,
            None,
        )
        .await
        .expect("failed to create mail context");

        Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            context,
            tmp_dir,
            core_account,
            core_session,
            mock_web_server,
            mail_context,
        })
    }

    /// Get the test user context.
    ///
    /// # Panics
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

    /// Get the test user mail context.
    ///
    /// # Panics
    pub async fn mail_user_context(&self) -> Arc<MailUserContext> {
        self.mail_context
            .user_context_from_session(&self.core_session)
            .await
            .expect("failed to create user context")
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
    /// # Panics
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
            .mount(self.mock_server())
            .await;
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
