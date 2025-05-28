use crate::datatypes::ProductUsedSpace;
use crate::db::account::{CoreAccount, CoreSession};
use crate::events::{Action, AddressEvent, ContactEmailEvent, ContactEvent};
use crate::models::{ModelExtension, User, UserSettings};
use crate::test_utils::account::{TEST_USER_ID, TEST_USER_MAIL, testdata_user_secret};
use crate::test_utils::utils::catch_all;
use crate::utils::MapVec;
use crate::{
    Context, CoreEvent, CoreEventSubscriberConnectionProvider, UserContext,
    UserDatabaseInitializer,
    db::account::SessionEncryptionKey,
    os::{InMemoryKeyChain, KeyChain, KeyChainExt},
};
use async_trait::async_trait;
use proton_core_api::auth::{Tokens, UserKeySecret};
use proton_core_api::services::proton::{
    Action as ApiAction, AddressEvent as ApiAddressEvent,
    ContactEmailEvent as ApiContactEmailEvent, ContactEvent as ApiContactEvent, User as ApiUser,
    UserSettings as ApiUserSettings,
};
use proton_core_api::services::proton::{EventId, SessionId, UserId};
use proton_core_api::session::{Config, Endpoint, EnvId};
use proton_core_api::status_observer::StatusObserver;
use proton_core_api::status_watcher::StatusWatcher;
use proton_event_loop::Event;
use proton_sqlite3::MigratorError;
use serde::Deserialize;
use stash::stash::{Stash, StashError};
use std::io::stdout;
use std::sync::Arc;
use std::sync::Weak;
use tempdir::TempDir;
use tracing::subscriber::set_global_default;
use tracing::{Level, info};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, registry};
use wiremock::MockServer;

pub trait BaseTestContext {
    /// Generate a test UID.
    #[must_use]
    fn test_uid() -> SessionId {
        SessionId::from("TEST_UID")
    }

    /// Generate a test user ID.
    #[must_use]
    fn test_user_id() -> UserId {
        UserId::from(TEST_USER_ID)
    }

    /// Generate a test user name or address.
    #[must_use]
    fn test_user_mail() -> String {
        TEST_USER_MAIL.to_owned()
    }

    /// Generate a test access token.
    #[must_use]
    fn test_acctok() -> String {
        String::from("ACCESSTOKEN")
    }

    /// Generate a test refresh token.
    #[must_use]
    fn test_reftok() -> String {
        String::from("REFRESHTOKEN")
    }

    /// Generate test scopes.
    #[must_use]
    fn test_scopes() -> Vec<String> {
        vec!["foo".to_owned(), "bar".to_owned()]
    }

    #[must_use]
    fn keychain() -> Arc<impl KeyChain> {
        Arc::new(InMemoryKeyChain::default())
    }

    #[must_use]
    fn encryption_key() -> SessionEncryptionKey {
        SessionEncryptionKey::random()
    }

    fn store_encryption_key_in_keychain(
        keychain: Arc<impl KeyChain>,
        encryption_key: SessionEncryptionKey,
    ) {
        keychain
            .store(encryption_key)
            .expect("failed to store in keychain");
    }

    #[must_use]
    fn api_config(mock_web_server: &MockServer) -> Config {
        Config {
            env_id: EnvId::new_custom(MockApiEnv::new(mock_web_server.uri()).with_path("/api")),
            ..Config::default()
        }
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
    pub tmp_dir: TempDir,
    pub core_account: CoreAccount,
    pub core_session: CoreSession,
    pub mock_web_server: Arc<MockServer>,
    key: SessionEncryptionKey,
}

impl BaseTestContext for TestContext {}

impl TestContext {
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.context
    }

    #[must_use]
    pub fn mock_server(&self) -> &MockServer {
        &self.mock_web_server
    }

    /// Create and initialize test context.
    pub async fn new() -> Arc<Self> {
        Self::_new(None, None, None).await
    }

    /// Create and initialize test context and override the default `user_key_secret` and `user_id`.
    pub async fn with_user_secret_and_user_id(
        user_key_secret: UserKeySecret,
        user_id: UserId,
        initializers: Option<Vec<Box<dyn UserDatabaseInitializer>>>,
    ) -> Arc<Self> {
        Self::_new(Some(user_key_secret), Some(user_id), initializers).await
    }

    /// Create and initialize test context and override the default `user_key_secret` and `user_id`.
    pub async fn with_initializers(
        initializers: Option<Vec<Box<dyn UserDatabaseInitializer>>>,
    ) -> Arc<Self> {
        Self::_new(None, None, initializers).await
    }

    async fn _new(
        user_key_secret: Option<UserKeySecret>,
        user_id: Option<UserId>,
        initializers: Option<Vec<Box<dyn UserDatabaseInitializer>>>,
    ) -> Arc<Self> {
        drop(set_global_default(
            registry()
                .with(EnvFilter::new("debug,stash=info"))
                .with(layer().with_writer(stdout.with_max_level(Level::TRACE))),
        ));

        let mock_web_server = Arc::new(MockServer::start().await);
        let tmp_dir = TempDir::new("account_test").expect("failed to create temp dir");
        info!("CORE TMP DIR = {:?}", tmp_dir.path());
        let keychain = Self::keychain();
        let api_config = Self::api_config(&mock_web_server);
        let key = Self::encryption_key();

        keychain
            .store(key.clone())
            .expect("failed to store in keychain");

        // Use the given data or fall back to the default
        let user_id = user_id.unwrap_or_else(Self::test_user_id);
        let user_key_secret = user_key_secret.unwrap_or_else(testdata_user_secret);

        let mut all_initializers: Vec<Box<dyn UserDatabaseInitializer>> =
            vec![TestCoreDatabaseInitializer.boxed()];

        if let Some(mut additional_initializers) = initializers {
            all_initializers.append(&mut additional_initializers);
        }

        // Create core test context
        let context = Context::new(
            tmp_dir.path(),
            tmp_dir.path(),
            keychain.clone(),
            all_initializers,
            api_config.clone(),
            None,
            tmp_dir.path().join("core-cache"),
            None,
            Some(tmp_dir.path().join("logs")),
        )
        .await
        .expect("failed to create core context");

        // Generate a fake session and write it to the database
        let (core_account, core_session) = Self::new_account_impl(
            &context,
            user_id.clone(),
            Self::test_uid(),
            user_key_secret,
            key.clone(),
        )
        .await;

        Arc::new_cyclic(|this| Self {
            this: Weak::clone(this),
            context,
            tmp_dir,
            core_account,
            core_session,
            mock_web_server,
            key,
        })
    }

    /// Creates a new account and a fake session.
    ///
    /// # Panics
    ///
    pub async fn new_account(
        &self,
        user_id: UserId,
        session_id: SessionId,
        user_key_secret: Option<UserKeySecret>,
    ) -> (CoreAccount, CoreSession) {
        let key = self.key.clone();
        let user_key_secret = user_key_secret.unwrap_or_else(testdata_user_secret);
        Self::new_account_impl(&self.context, user_id, session_id, user_key_secret, key).await
    }

    async fn new_account_impl(
        context: &Context,
        user_id: UserId,
        session_id: SessionId,
        user_key_secret: UserKeySecret,
        key: SessionEncryptionKey,
    ) -> (CoreAccount, CoreSession) {
        let (core_account, core_session) = {
            // Create a temporary stash just to insert the fake data.
            let mut tether = context.account_stash().connection();
            tether
                .tx::<_, _, StashError>(async |tx| {
                    // Create
                    let account = CoreAccount::new(user_id.clone(), Self::test_user_mail())
                        .with_save(tx)
                        .await
                        .expect("fake account should save");

                    // Create a auth session.
                    let tokens = Tokens::access(
                        Self::test_acctok(),
                        Self::test_reftok(),
                        Self::test_scopes(),
                    );

                    // Create a fake session.
                    let session = CoreSession::new(user_id.clone(), session_id, &tokens, &key)
                        .expect("session should be created")
                        .with_key_secret(&user_key_secret, &key)
                        .expect("key secret should be set")
                        .with_save(tx)
                        .await
                        .expect("fake session should save");
                    Ok((account, session))
                })
                .await
                .expect("failed to create transaction")
        };
        (core_account, core_session)
    }

    /// Get the test user context.
    ///
    /// # Panics
    pub async fn user_context(&self) -> Arc<UserContext> {
        self.context
            .user_context_from_session(
                &self.core_session,
                Some(StatusWatcher::with_observer(StatusObserver::test())),
            )
            .await
            .expect("failed to create user context")
    }

    /// Get the core context
    ///
    #[must_use]
    pub fn core_context(&self) -> &Arc<Context> {
        &self.context
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
    ///
    pub async fn catch_all(&self) {
        catch_all(self.mock_server()).await;
    }
}

#[async_trait]
impl CoreEventSubscriberConnectionProvider for TestContext {
    async fn get_user_id_and_db_connection(&self) -> anyhow::Result<(UserId, Stash)> {
        let user_ctx = self.user_context().await;

        Ok((user_ctx.user_id().clone(), user_ctx.stash().clone()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TestApiCoreEvent {
    pub event_id: EventId,
    pub action: ApiAction,
    pub address: Option<Vec<ApiAddressEvent>>,
    pub contact_emails: Option<Vec<ApiContactEmailEvent>>,
    pub contacts: Option<Vec<ApiContactEvent>>,
    pub has_more: bool,
    pub user: Option<ApiUser>,
    pub user_settings: Option<ApiUserSettings>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestCoreEvent {
    pub event_id: EventId,
    pub action: Action,
    pub address: Option<Vec<AddressEvent>>,
    pub contact_emails: Option<Vec<ContactEmailEvent>>,
    pub contacts: Option<Vec<ContactEvent>>,
    pub has_more: bool,
    pub user: Option<User>,
    pub user_settings: Option<UserSettings>,
}

impl Event for TestCoreEvent {
    type Response = TestApiCoreEvent;

    fn event_id(&self) -> &EventId {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        false
    }

    fn is_refresh(&self) -> bool {
        false
    }
}

impl From<TestApiCoreEvent> for TestCoreEvent {
    fn from(value: TestApiCoreEvent) -> Self {
        Self {
            event_id: value.event_id,
            action: value.action.into(),
            address: value.address.map_vec(),
            contact_emails: value.contact_emails.map_vec(),
            contacts: value.contacts.map_vec(),
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

    fn get_core_event_addresses(&self) -> Option<&[AddressEvent]> {
        self.address.as_deref()
    }
    fn get_core_event_addresses_mut(&mut self) -> Option<&mut [AddressEvent]> {
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
            event_id: EventId::from("test_event"),
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

#[must_use]
#[derive(Debug)]
pub struct MockApiEnv {
    host: Endpoint,
    path: String,
}

impl MockApiEnv {
    /// Create a new `MockApiEnv` with the given host.
    ///
    /// # Panics
    ///
    /// Panics if the given host is not a valid URL.
    pub fn new(host: impl AsRef<str>) -> Self {
        Self {
            host: host.as_ref().parse().expect("URL must be valid"),
            path: String::default(),
        }
    }

    pub fn with_path(self, path: impl AsRef<str>) -> Self {
        let path = path.as_ref().to_owned();

        Self { path, ..self }
    }
}

const _: () = {
    use proton_core_api::session::{AppVersion, Env, Server, TlsPinSet};

    impl Env for MockApiEnv {
        fn servers(&self, _: &AppVersion) -> Vec<Server> {
            vec![Server::new(self.host.clone(), self.path.clone())]
        }

        fn pins(&self, _: &Server) -> Option<TlsPinSet> {
            None
        }
    }
};
