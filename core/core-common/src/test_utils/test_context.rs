use crate::db::account::{CoreAccount, CoreSession};
use crate::event_loop::EventPollMode;
use crate::models::ModelExtension;
use crate::test_utils::account::{TEST_USER_ID, TEST_USER_MAIL, testdata_user_secret};
use crate::test_utils::utils::catch_all;
use crate::{
    Context, UserContext, UserDatabaseInitializer,
    db::account::SessionEncryptionKey,
    os::{InMemoryKeyChain, KeyChain, KeyChainExt},
};
use proton_core_api::auth::{Tokens, UserKeySecret};
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::{Config, Endpoint, EnvId};
use proton_core_api::status_observer::StatusObserver;
use proton_core_api::status_watcher::StatusWatcher;
use proton_sqlite3::MigratorError;
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

        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            original_hook(info);
            std::process::exit(-1);
        }));

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
            None,
            "v1",
            tmp_dir.path().join("core-cache"),
            None,
            Some(tmp_dir.path().join("logs")),
            EventPollMode::Manual,
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
        catch_all(&self.mock_web_server).await;
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
