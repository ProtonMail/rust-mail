use proton_api_core::auth::UserKeySecret;
use proton_api_core::services::proton::common::UserId;
use proton_api_core::status_watcher::StatusWatcher;
use proton_core_common::db::account::{CoreAccount, CoreSession};
use proton_core_common::UserDatabaseInitializer;
use proton_core_test_utils::test_context::{BaseTestContext, TestContext};
use proton_mail_common::context::MailUserDatabaseInitializer;
use proton_mail_common::{MailContext, MailUserContext};
pub use secrecy::{ExposeSecret, SecretString as RealSecretString};
use std::sync::Arc;
use tempdir::TempDir;
use wiremock::matchers::any;
use wiremock::MockServer;
use wiremock::{Mock, Request};

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
        let mail_cache_path = tmp_dir.path().join("mail-cache");
        let mail_cache_size = 2 << 29; // 512MiB

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

    /// Get the test user context.
    ///
    /// # Panics
    /// Get the test user mail context.
    pub async fn mail_user_context(&self) -> Arc<MailUserContext> {
        self.mail_context
            .user_context_from_session(&self.core_session, Some(StatusWatcher::test()))
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
                    String::from_utf8(request.body.clone()).unwrap(),
                );
            })
            .named(function_name!())
            .mount(self.mock_server())
            .await;
    }
}
