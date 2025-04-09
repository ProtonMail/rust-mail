use crate::services::proton::ProtonCore;
use crate::services::proton::common::Timeouts;
use crate::{connection_status::ConnectionStatus, services::proton::Proton};
use derive_more::Deref;
use muon::common::{BoxFut, RetryPolicy, Sender, SenderLayer};
use muon::error::ErrorKind;
use muon::util::DurationExt;
use muon::{Error as MuonError, ProtonRequest, ProtonResponse, Result as MuonResult};
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, watch};
use tokio::task::JoinHandle;
use tracing::trace;

mod fixed_queue;

use fixed_queue::StatusChanges;

type StatusJoinHandle = JoinHandle<()>;

const UP_TO_DATE_SECONDS: u64 = 6;
static STATUS: LazyLock<Arc<RwLock<Status>>> = LazyLock::new(|| {
    Arc::new(RwLock::new(Status {
        status: ConnectionStatus::Online,
        last_check: Instant::now()
            .checked_sub(Duration::from_secs(UP_TO_DATE_SECONDS + 1))
            .unwrap(),
    }))
});

/// The connection status and the last time it was checked.
#[derive(Clone, Debug)]
struct Status {
    status: ConnectionStatus,
    last_check: Instant,
}

impl Deref for Status {
    type Target = ConnectionStatus;

    fn deref(&self) -> &Self::Target {
        &self.status
    }
}

/// A background ping request.
///
/// It keeps track of the request background task and the receiver to know when it's finished.
///
#[derive(Debug)]
struct BackgroundPing {
    request: StatusJoinHandle,
}

impl BackgroundPing {
    fn is_finished(&self) -> bool {
        self.request.is_finished()
    }
}

/// Configuration for the `StatusObserver`.
///
#[derive(Clone, Debug)]
struct StatusObserverConfig {
    /// Forground ping's retry policy.
    fg_retry: RetryPolicy,
    /// Forground ping's timeout.
    fg_timeout: Duration,
    /// Background ping's retry policy.
    bg_retry: RetryPolicy,
    /// Background ping's timeout.
    bg_timeout: Duration,
    /// Number of seconds before the status is considered stale.
    up_to_date: Duration,
}

impl Default for StatusObserverConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusObserverConfig {
    /// Create a new `StatusObserverConfig` with default production values.
    fn new() -> Self {
        Self {
            up_to_date: Duration::from_secs(UP_TO_DATE_SECONDS),
            fg_retry: RetryPolicy::default().never(),
            fg_timeout: Timeouts::TWO_SECONDS,
            bg_retry: RetryPolicy::default()
                .max_count(2)
                .max_delay(5.s())
                .iter_mul(1.0),
            bg_timeout: Timeouts::QUARTER_MINUTE,
        }
    }

    /// Create a new `StatusObserverConfig` with default test values.
    fn test() -> Self {
        // Make single requests
        let test_retry_policy = RetryPolicy::default().never();
        let fg_timeout = Timeouts::ONE_SECOND;
        let bg_timeout = Timeouts::ONE_SECOND * 2;
        let up_to_date = Duration::from_secs(2);

        Self {
            up_to_date,
            fg_retry: test_retry_policy,
            fg_timeout,
            bg_retry: test_retry_policy,
            bg_timeout,
        }
    }
}

/// A `StatusObserver` that will keep track of the connection status.
///
/// It will ping the server to get the current status if the status is stale.
/// If the status is `Offline`, it will start a background check.
/// The status is initialized to `Online`.
/// With the default configuration, the last check is initialized to `Instant::now() - UP_TO_DATE_SECONDS` to make it stale.
///
#[must_use]
#[derive(Clone, Debug)]
pub struct StatusObserver {
    status: Arc<RwLock<Status>>,
    request: Arc<RwLock<Option<BackgroundPing>>>,
    config: StatusObserverConfig,
    on_update: watch::Sender<ConnectionStatus>,
    past_statuses: StatusChanges,
}

impl StatusObserver {
    /// Create a new `StatusObserver`.
    ///
    /// The status is initialized to `Online`.
    /// The last check is initialized to `Instant::now() - UP_TO_DATE_SECONDS` to make it stale.
    ///
    /// # Panics
    ///
    /// Should not panic as `checked_sub` is subtracting a value that is within the range of `Instant`.
    /// If it does, it's a bug.
    ///
    pub fn new() -> Self {
        let (on_update, _) = watch::channel(ConnectionStatus::Online);

        Self {
            status: Arc::clone(&STATUS),
            request: Arc::new(RwLock::new(None)),
            config: StatusObserverConfig::default(),
            on_update,
            past_statuses: StatusChanges::new(NonZeroUsize::new(3).unwrap()),
        }
    }

    /// Create a new test `StatusObserver` without shared state.
    ///
    /// The status is initialized to `Online`.
    /// The last check is initialized to `Instant::now() - UP_TO_DATE_SECONDS` to make it stale.
    ///
    /// # Panics
    ///
    /// Should not panic as `checked_sub` is subtracting a value that is within the range of `Instant`.
    /// If it does, it's a bug.
    ///
    #[cfg(any(test, debug_assertions))]
    pub fn test() -> Self {
        let (on_update, _) = watch::channel(ConnectionStatus::Online);
        let config = StatusObserverConfig::test();
        let stale_instant = Instant::now()
            .checked_sub(Duration::from_secs(config.up_to_date.as_secs() + 1))
            .unwrap();

        Self {
            status: Arc::new(RwLock::new(Status {
                status: ConnectionStatus::Online,
                last_check: stale_instant,
            })),
            request: Arc::new(RwLock::new(None)),
            config,
            on_update,
            past_statuses: StatusChanges::new(NonZeroUsize::new(3).unwrap()),
        }
    }

    /// Sets the number of seconds before the status is considered stale.
    ///
    /// The status is initialized to `Online`.
    /// The last check is initialized to `Instant::now() - UP_TO_DATE_SECONDS` to make it stale.
    ///
    /// # Panics
    ///
    /// Should not panic as `checked_sub` is subtracting a value that is within the range of `Instant`.
    /// If it does, it's a bug.
    ///
    #[cfg(any(test, debug_assertions))]
    #[must_use]
    pub async fn set_up_to_date(&mut self, up_to_date: Duration) {
        let stale_instant = Instant::now()
            .checked_sub(Duration::from_secs(up_to_date.as_secs() + 1))
            .unwrap();
        self.status.write().await.last_check = stale_instant;
        self.config.up_to_date = up_to_date;
    }

    /// Get the current status of the connection.
    /// If the status is stale, it will ping the server to get the current status.
    /// If the status is `Offline`, it will start a background check.
    ///
    pub async fn status(&self, api: Proton) -> ConnectionStatus {
        if !self.is_up_to_date().await {
            let request_finished = self
                .request
                .read()
                .await
                .as_ref()
                .filter(|ping| ping.is_finished())
                .is_some();

            if request_finished {
                drop(self.request.write().await.take());
            }

            let was_online_most_of_the_time = self.past_statuses.was_online_most_of_the_time();

            if self.get_status().await.is_offline() && was_online_most_of_the_time {
                Self::ping(api.clone(), self.config.fg_timeout, self.config.fg_retry).await;
            } else {
                self.background_check(api.clone()).await;
            }
        } else if self.get_status().await.is_offline() {
            self.background_check(api).await;
        }

        self.get_status().await
    }

    /// Peek in `update` method
    ///
    /// Expose internal information about status updates
    ///
    #[must_use]
    pub fn on_updates(&self) -> watch::Receiver<ConnectionStatus> {
        self.on_update.subscribe()
    }

    async fn update(&self, status: ConnectionStatus) {
        let mut self_status = self.status.write().await;

        if self_status.status != status {
            self.on_update.send_replace(status);
        }

        self.past_statuses.push(self_status.status);
        self_status.last_check = Instant::now();
        self_status.status = status;

        trace!("Status has been updated to {:?}", status);
    }

    async fn ping(api: Proton, timeout: Duration, retry: RetryPolicy) {
        let _ = api.get_tests_ping(Some(timeout), Some(retry)).await;
    }

    #[allow(clippy::let_underscore_future)]
    async fn background_check(&self, api: Proton) {
        let request_is_not_running = self.request.read().await.is_none();
        if request_is_not_running {
            let timeout = self.config.bg_timeout;
            let retry = self.config.bg_retry;
            let ping = BackgroundPing {
                request: tokio::spawn(async move {
                    Self::ping(api, timeout, retry).await;
                }),
            };
            let _ = self.request.write().await.insert(ping);
        }
    }

    async fn is_up_to_date(&self) -> bool {
        self.status.read().await.last_check.elapsed() < self.config.up_to_date
    }

    async fn get_status(&self) -> ConnectionStatus {
        self.status.read().await.status
    }
}

impl Default for StatusObserver {
    fn default() -> Self {
        Self::new()
    }
}

/// A type that wraps a [`StatusObserver`] and to implement the [`SenderLayer`] trait.
#[derive(Debug, Deref)]
pub struct StatusObserverLayer(StatusObserver);

impl StatusObserverLayer {
    #[must_use]
    pub fn new(observer: StatusObserver) -> Self {
        Self(observer)
    }

    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        match inner.send(req).await {
            Ok(resp) => {
                self.on_recv_ok(&resp).await;

                Ok(resp)
            }

            Err(error) => {
                self.on_recv_err(&error).await;

                Err(error)
            }
        }
    }

    async fn on_recv_err(&self, error: &MuonError) {
        match error.kind() {
            ErrorKind::Tls | ErrorKind::Resolve | ErrorKind::Dial | ErrorKind::Send => {
                self.update(ConnectionStatus::Offline).await;
            }

            ErrorKind::Connect | ErrorKind::Closed => {
                self.update(ConnectionStatus::ServerUnreachable).await;
            }

            _ => {}
        }
    }

    async fn on_recv_ok(&self, resp: &ProtonResponse) {
        if resp.is(429) || resp.status().is_server_error() {
            self.update(ConnectionStatus::ServerUnreachable).await;
        } else {
            self.update(ConnectionStatus::Online).await;
        }
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for StatusObserverLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}
