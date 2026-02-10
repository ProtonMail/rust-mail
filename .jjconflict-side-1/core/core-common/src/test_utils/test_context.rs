use crate::Origin;
use crate::datatypes::ApiConfig;
use crate::db::account::{CoreAccount, CoreSession};
use crate::event_loop::EventPollMode;
use crate::event_loop::event_source::CoreEventSource;
use crate::models::ModelExtension;
use crate::services::global_feature_flags::FeatureFlagsBackgroundTask;
use crate::test_utils::account::{TEST_USER_ID, TEST_USER_MAIL, testdata_user_secret};
use crate::test_utils::utils::mock_auth_endpoints;
use crate::{
    Context, UserContext, UserDatabaseInitializer,
    db::account::SessionEncryptionKey,
    os::{InMemoryKeyChain, KeyChain, KeyChainExt},
};
use proton_core_api::auth::{Tokens, UserKeySecret};
use proton_core_api::exports::RetryPolicy;
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::{AppVersion, Env, Server};
use proton_core_api::session::{Endpoint, EnvId};
use proton_event_loop::v6::{EventSource, EventSubscriberResult};
use proton_issue_reporter_service::{IssueReporter, NoopIssueReporter};
use proton_log_service::LogService;
use proton_sqlite3::MigratorError;
use stash::UserDb;
use stash::stash::{Stash, StashError};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime;
use tracing::info;
use tracing::subscriber::set_global_default;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, registry};
use wiremock::MockServer;

pub trait BaseTestContext {
    #[must_use]
    fn test_uid() -> SessionId {
        SessionId::from("TEST_UID")
    }

    #[must_use]
    fn test_user_id() -> UserId {
        UserId::from(TEST_USER_ID)
    }

    #[must_use]
    fn test_user_mail() -> String {
        TEST_USER_MAIL.to_owned()
    }

    #[must_use]
    fn test_acctok() -> String {
        String::from("ACCESSTOKEN")
    }

    #[must_use]
    fn test_reftok() -> String {
        String::from("REFRESHTOKEN")
    }

    #[must_use]
    fn test_scopes() -> Vec<String> {
        vec!["foo".to_owned(), "bar".to_owned(), "full".to_owned()]
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
    fn api_config(mock_web_server: &MockServer) -> ApiConfig {
        ApiConfig {
            env_id: EnvId::new_custom(MockApiEnv::new(mock_web_server.uri()).with_path("/api")),
            ..ApiConfig::default()
        }
    }
}

struct TestCoreDatabaseInitializer;

#[async_trait::async_trait]
impl UserDatabaseInitializer for TestCoreDatabaseInitializer {
    async fn initialize(&self, _stash: &Stash<UserDb>) -> Result<(), MigratorError> {
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
    pub mock_web_server: MockServer,
    key: SessionEncryptionKey,
}

impl BaseTestContext for TestContext {}

#[must_use]
pub fn test_network_monitor_service_config() -> proton_network_monitor_service::Config {
    let never = RetryPolicy::default().never();
    proton_network_monitor_service::Config {
        immediate: proton_network_monitor_service::ImmediateConfig {
            retry_policy: never,
            command_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(2),
            retry_interval: Duration::from_secs(0),
        },
        background: proton_network_monitor_service::BackgroundConfig {
            retry_policy: never,
            timeout: Duration::from_secs(2),
            infinite_checks: false,
        },
    }
}

impl TestContext {
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.context
    }

    #[must_use]
    pub fn mock_server(&self) -> &MockServer {
        &self.mock_web_server
    }

    pub async fn new() -> Arc<Self> {
        Self::_new(None, None, None, None).await
    }

    pub async fn with_user_secret_and_user_id(
        user_key_secret: UserKeySecret,
        user_id: UserId,
        initializers: Option<Vec<Box<dyn UserDatabaseInitializer>>>,
    ) -> Arc<Self> {
        Self::_new(Some(user_key_secret), Some(user_id), initializers, None).await
    }

    pub async fn with_initializers(
        initializers: Option<Vec<Box<dyn UserDatabaseInitializer>>>,
    ) -> Arc<Self> {
        Self::_new(None, None, initializers, None).await
    }

    pub async fn with_issue_reporter(reporter: Arc<dyn IssueReporter>) -> Arc<Self> {
        Self::_new(None, None, None, Some(reporter)).await
    }

    async fn _new(
        user_key_secret: Option<UserKeySecret>,
        user_id: Option<UserId>,
        initializers: Option<Vec<Box<dyn UserDatabaseInitializer>>>,
        issue_reporter: Option<Arc<dyn IssueReporter>>,
    ) -> Arc<Self> {
        _ = set_global_default(
            registry()
                .with(EnvFilter::new("debug"))
                .with(layer().with_test_writer()),
        );

        let mock_web_server = MockServer::start().await;
        mock_auth_endpoints(&mock_web_server).await;
        let tmp_dir = TempDir::new().expect("failed to create temp dir");
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

        let log_config = proton_log_service::Config::builder()
            .directory(tmp_dir.path().into())
            .max_log_size(20 * 1024 * 1024)
            .name("log".into())
            .build();

        let context = Context::new(
            Origin::App,
            runtime::Handle::current(),
            tmp_dir.path(),
            tmp_dir.path(),
            keychain.clone(),
            all_initializers,
            api_config.clone(),
            None,
            None,
            tmp_dir.path().join("core-cache"),
            LogService::new(log_config),
            EventPollMode::Manual,
            test_network_monitor_service_config(),
            issue_reporter.unwrap_or(Arc::new(NoopIssueReporter)),
            FeatureFlagsBackgroundTask::Disabled,
        )
        .await
        .expect("failed to create core context");

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
            let mut tether = context.account_stash().connection().await.unwrap();
            tether
                .tx::<_, _, StashError>(async |tx| {
                    // Create
                    let account = CoreAccount::new(user_id.clone(), Self::test_user_mail())
                        .with_insert(tx)
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

    pub async fn user_context(&self) -> Arc<UserContext> {
        self.context
            .user_context_from_session(&self.core_session)
            .await
            .expect("failed to create user context")
    }

    #[must_use]
    pub fn core_context(&self) -> &Arc<Context> {
        &self.context
    }
}

#[allow(async_fn_in_trait)]
pub trait UserContextTestExtension {
    async fn apply_event(
        self: &Arc<Self>,
        event: &<CoreEventSource as EventSource>::Event,
    ) -> EventSubscriberResult<()>;
}

impl UserContextTestExtension for UserContext {
    async fn apply_event(
        self: &Arc<Self>,
        event: &<CoreEventSource as EventSource>::Event,
    ) -> EventSubscriberResult<()> {
        use proton_event_loop::v6::EventSubscriber;
        let subscriber = self.event_subscriber();
        let mut cache = <CoreEventSource as EventSource>::Cache::default();
        subscriber.on_event(event, &mut cache).await
    }
}

#[must_use]
#[derive(Debug)]
pub struct MockApiEnv {
    host: Endpoint,
    path: String,
}

impl MockApiEnv {
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

impl Env for MockApiEnv {
    fn servers(&self, _: &AppVersion) -> Vec<Server> {
        vec![Server::new(self.host.clone(), self.path.clone())]
    }
}
