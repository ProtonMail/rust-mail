use crate::actions::draft::SEND_ACTION_GROUP;
use crate::context::{MailUserDatabaseInitializer, ShouldInitializeMailUserContext};
use crate::events::MailEvent;
use crate::{MailContext, MailContextResult, MailUserContext};
use proton_action_queue::action::ActionGroup;
use proton_action_queue::queue::{QueuedActionState, QueuedResult};
use proton_core_api::auth::UserKeySecret;
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::services::proton::UserId;
use proton_core_common::UserDatabaseInitializer;
use proton_core_common::db::account::{CoreAccount, CoreSession};
use proton_core_common::test_utils::test_context::{BaseTestContext, TestContext};
use proton_event_loop::subscriber::SubscriberError;
pub use secrecy::{ExposeSecret, SecretString as RealSecretString};
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempdir::TempDir;
use tracing::info;
use wiremock::matchers::any;
use wiremock::{Mock, Request, Respond};
use wiremock::{MockServer, ResponseTemplate};

/// Test context for mail tests.
///
/// This struct provides a test context with a handcrafted new session, so that
/// we can bypass authentication. It also spins up a mock server.
///
/// TODO: Remove more shared code as part of ET-1381. Use `TestContext` instead.
#[allow(dead_code)]
pub struct MailTestContext {
    pub core_test_context: Arc<TestContext>,
    pub mail_context: Arc<MailContext>,
    pub mock_web_server: Arc<MockServer>,
    tmp_dir: TempDir,
    core_account: CoreAccount,
    core_session: CoreSession,
}

impl Deref for MailTestContext {
    type Target = TestContext;
    fn deref(&self) -> &Self::Target {
        &self.core_test_context
    }
}

impl BaseTestContext for MailTestContext {}

impl MailTestContext {
    #[must_use]
    pub fn context(&self) -> &Arc<MailContext> {
        &self.mail_context
    }

    #[must_use]
    pub fn core_test_context(&self) -> &TestContext {
        &self.core_test_context
    }

    #[must_use]
    pub fn mock_server(&self) -> &MockServer {
        &self.mock_web_server
    }

    /// Create and initialize test context.
    pub async fn new() -> Self {
        Self::_new(None, None).await
    }

    /// Create and initialize test context and override the default `user_key_secret` and `user_id`.
    pub async fn with_user_secret_and_user_id(
        user_key_secret: UserKeySecret,
        user_id: UserId,
    ) -> Self {
        Self::_new(Some(user_key_secret), Some(user_id)).await
    }

    /// Function to create `MailContext` instance based on parameters provided.
    /// TODO: ET-1381, decouple Mail database initialization.
    async fn _new(user_key_secret: Option<UserKeySecret>, user_id: Option<UserId>) -> Self {
        let initializers: Option<Vec<Box<dyn UserDatabaseInitializer>>> =
            Some(vec![Box::new(MailUserDatabaseInitializer {})]);

        let core_test_context = if let (Some(secret), Some(id)) = (user_key_secret, user_id) {
            TestContext::with_user_secret_and_user_id(secret, id, initializers).await
        } else {
            TestContext::with_initializers(initializers).await
        };

        let tmp_dir = TempDir::new("pmc_test").expect("failed to create temp dir");
        info!("MAIL TMP DIR = {:?}", tmp_dir.path());
        let mail_cache_path = tmp_dir.path().join("mail-cache");
        let mail_cache_size = 2 << 17; // 256KiB

        let mail_context = MailContext::new_with_core_context(
            core_test_context.context.clone(),
            mail_cache_path,
            mail_cache_size,
        )
        .await
        .expect("failed to create mail context");

        let mock_web_server = core_test_context.mock_web_server.clone();
        let core_account = core_test_context.core_account.clone();
        let core_session = core_test_context.core_session.clone();

        Self {
            core_test_context,
            mail_context,
            mock_web_server,
            tmp_dir,
            core_account,
            core_session,
        }
    }

    /// User context that is not initialized. Use to setup the mocks.
    ///
    /// # Warning
    ///
    /// Since asking for a new context does not initialize it (we are reusing them),
    /// it is programmers responsibility to initialize context manually afterwards.
    ///
    pub async fn uninitialized_mail_user_context(&self) -> Arc<MailUserContext> {
        let ctx = self
            .mail_context
            .user_context_from_session(&self.core_session, ShouldInitializeMailUserContext::No)
            .await
            .expect("failed to create user context");

        // Disable auto queue executor as we don't want these to interfere with our test execution.
        ctx.queues().terminate();
        ctx
    }

    /// Initialize context that was previously not initialized.
    ///
    /// Only use in the pair with [`Self::uninitialized_mail_user_context`].
    ///
    pub async fn initialize_uninitialized_ctx(&self, ctx: &Arc<MailUserContext>) {
        MailUserContext::initialize_async(ctx.clone())
            .await
            .expect("Failed to initialize");
    }

    /// Get the test user context.
    /// Has to be called **AFTER** setting up the API mocks
    ///
    pub async fn mail_user_context(&self) -> Arc<MailUserContext> {
        let ctx = self
            .mail_context
            .user_context_from_session(&self.core_session, ShouldInitializeMailUserContext::Yes)
            .await
            .expect("failed to create user context");

        // Disable auto queue executor as we don't want these to interfere with our test execution.
        ctx.queues().terminate();
        ctx
    }

    /// Get the test user context.
    /// Has to be called **AFTER** setting up the API mocks
    ///
    /// # Errors
    ///
    /// Returns an error if context could not be initialized.
    pub async fn try_mail_user_context(&self) -> MailContextResult<Arc<MailUserContext>> {
        let ctx = self
            .mail_context
            .user_context_from_session(&self.core_session, ShouldInitializeMailUserContext::Yes)
            .await?;

        // Disable auto queue executor as we don't want these to interfere with our test execution.
        ctx.queues().terminate();

        Ok(ctx)
    }

    /// Get the test user context but only if its initialized
    ///
    pub async fn initialized_mail_user_context(&self) -> Option<Arc<MailUserContext>> {
        let ctx = self
            .mail_context
            .initialized_user_context_from_session(&self.core_session)
            .await
            .unwrap()?;

        // Disable auto queue executor as we don't want these to interfere with our test execution.
        ctx.queues().terminate();

        Some(ctx)
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
    #[function_name::named]
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
                    String::from_utf8_lossy(&request.body)
                );
            })
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}

/// Extension trait to help with the action queue.
#[allow(async_fn_in_trait)]
pub trait MailUserContextTestExtension {
    /// Execute a single action from the [`Queue`] with the default action group.
    async fn execute_single_action(&self) -> QueuedResult<Option<QueuedActionState>> {
        self.execute_single_action_with_group(ActionGroup::default())
            .await
    }

    /// Execute a single action from the [`Queue`] with a given `action_group`.
    async fn execute_single_action_with_group(
        &self,
        action_group: ActionGroup,
    ) -> QueuedResult<Option<QueuedActionState>>;

    /// Execute all available actions from the [`Queue`] with the default action group.
    async fn execute_all_actions(&self) -> QueuedResult<usize> {
        self.execute_all_actions_with_group(ActionGroup::default())
            .await
    }

    /// Execute all available actions from the [`Queue`] with the given `action_group`.
    async fn execute_all_actions_with_group(
        &self,
        action_group: ActionGroup,
    ) -> QueuedResult<usize>;

    /// Execute a single action from the [`Queue`] with the send action group.
    async fn execute_single_send_action(&self) -> QueuedResult<Option<QueuedActionState>> {
        self.execute_single_action_with_group(SEND_ACTION_GROUP)
            .await
    }

    /// Execute all available actions from the [`Queue`] with the Send action group.
    async fn execute_all_send_actions(&self) -> QueuedResult<usize> {
        self.execute_all_actions_with_group(SEND_ACTION_GROUP).await
    }

    async fn wait_for(&self, timeout: Option<Duration>, fun: impl Fn(ConnectionStatus) -> bool);

    async fn apply_event(&self, event: MailEvent) -> Result<(), SubscriberError>;
}

impl MailUserContextTestExtension for MailUserContext {
    async fn execute_single_action_with_group(
        &self,
        action_group: ActionGroup,
    ) -> QueuedResult<Option<QueuedActionState>> {
        let executor = self.action_queue().new_executor_with_group(action_group);
        executor.execute_one().await
    }

    async fn execute_all_actions_with_group(
        &self,
        action_group: ActionGroup,
    ) -> QueuedResult<usize> {
        let executor = self.action_queue().new_executor_with_group(action_group);
        executor.execute_all().await
    }

    async fn wait_for(&self, timeout: Option<Duration>, fun: impl Fn(ConnectionStatus) -> bool) {
        if let Some(timeout) = timeout {
            tokio::time::timeout(timeout, wait_for_impl(self, fun))
                .await
                .unwrap();
        } else {
            wait_for_impl(self, fun).await;
        }
    }

    async fn apply_event(&self, event: MailEvent) -> Result<(), SubscriberError> {
        use proton_event_loop::Subscriber;
        self.event_subscriber().on_events(&mut [event]).await
    }
}

async fn wait_for_impl(user_ctx: &MailUserContext, fun: impl Fn(ConnectionStatus) -> bool) {
    while !fun(user_ctx.network_monitor_service().combined_status()) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Whenever we need to test a specific response pattern.
/// Example: Service is unavailable for the first 3 times.
pub struct RespondNthTime {
    count: AtomicUsize,
    max: usize,
    before: ResponseTemplate,
    after: ResponseTemplate,
}

impl RespondNthTime {
    pub fn new(max: usize, before: ResponseTemplate, after: ResponseTemplate) -> Self {
        Self {
            count: AtomicUsize::new(0),
            max,
            before,
            after,
        }
    }
}
impl Respond for RespondNthTime {
    fn respond(&self, _request: &wiremock::Request) -> ResponseTemplate {
        let time = self.count.fetch_add(1, Ordering::SeqCst);
        if time < self.max {
            return self.before.clone();
        }

        self.after.clone()
    }
}
