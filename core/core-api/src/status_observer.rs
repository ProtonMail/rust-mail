mod fixed_queue;

use self::fixed_queue::StatusChanges;
use crate::services::proton::ProtonCore;
use crate::services::proton::common::Timeouts;
use crate::{connection_status::ConnectionStatus, services::proton::Proton};
use derive_more::Deref;
use muon::common::{BoxFut, RetryPolicy, Sender, SenderLayer};
use muon::error::ErrorKind;
use muon::util::DurationExt;
use muon::{Error as MuonError, ProtonRequest, ProtonResponse, Result as MuonResult};
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::trace;

const UP_TO_DATE_SECONDS: u64 = 6;

static CACHE: LazyLock<Arc<RwLock<CachedStatus>>> = LazyLock::new(|| {
    Arc::new(RwLock::new(CachedStatus {
        status: ConnectionStatus::Online,
        checked_at: None,
    }))
});

#[derive(Clone, Debug)]
struct CachedStatus {
    status: ConnectionStatus,
    checked_at: Option<Instant>,
}

impl Deref for CachedStatus {
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
    request: JoinHandle<()>,
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
    #[cfg(any(test, debug_assertions))]
    fn test() -> Self {
        let never = RetryPolicy::default().never();
        let fg_timeout = Timeouts::ONE_SECOND;
        let bg_timeout = Timeouts::ONE_SECOND * 2;
        let up_to_date = Duration::from_secs(2);

        Self {
            up_to_date,
            fg_retry: never,
            fg_timeout,
            bg_retry: never,
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
    cache: Arc<RwLock<CachedStatus>>,
    ping: Arc<RwLock<Option<BackgroundPing>>>,
    config: StatusObserverConfig,
    status_tx: watch::Sender<ConnectionStatus>,
    history: StatusChanges,
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
        let (status_tx, _) = watch::channel(ConnectionStatus::Online);

        Self {
            cache: Arc::clone(&CACHE),
            ping: Arc::new(RwLock::new(None)),
            config: StatusObserverConfig::default(),
            status_tx,
            history: StatusChanges::new(NonZeroUsize::new(3).unwrap()),
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
        let (status_tx, _) = watch::channel(ConnectionStatus::Online);
        let config = StatusObserverConfig::test();

        Self {
            cache: Arc::new(RwLock::new(CachedStatus {
                status: ConnectionStatus::Online,
                checked_at: None,
            })),
            ping: Arc::new(RwLock::new(None)),
            config,
            status_tx,
            history: StatusChanges::new(NonZeroUsize::new(3).unwrap()),
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
    pub fn set_up_to_date(&mut self, up_to_date: Duration) {
        let stale_instant = Instant::now()
            .checked_sub(Duration::from_secs(up_to_date.as_secs() + 1))
            .unwrap();

        self.cache.write().checked_at = Some(stale_instant);
        self.config.up_to_date = up_to_date;
    }

    /// Get the current status of the connection.
    /// If the status is stale, it will ping the server to get the current status.
    /// If the status is `Offline`, it will start a background check.
    ///
    pub async fn status(&self, api: Proton) -> ConnectionStatus {
        if !self.is_cache_fresh() {
            if self.get_cached_status().is_offline() && self.history.was_online_most_of_the_time() {
                Self::ping(api.clone(), self.config.fg_timeout, self.config.fg_retry).await;
            } else {
                self.spawn_ping(api.clone());
            }
        } else if self.get_cached_status().is_offline() {
            self.spawn_ping(api);
        }

        self.get_cached_status()
    }

    /// Subscribes to changes of the connection status.
    ///
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<ConnectionStatus> {
        self.status_tx.subscribe()
    }

    fn update(&self, new: ConnectionStatus) {
        let mut cache = self.cache.write();

        self.status_tx.send_if_modified(|old| {
            if new == *old {
                false
            } else {
                *old = new;
                true
            }
        });

        self.history.push(cache.status);

        cache.status = new;
        cache.checked_at = Some(Instant::now());

        trace!("Status has been updated to {:?}", new);
    }

    async fn ping(api: Proton, timeout: Duration, retry: RetryPolicy) {
        let _ = api.get_tests_ping(Some(timeout), Some(retry)).await;
    }

    fn spawn_ping(&self, api: Proton) {
        let mut ping = self.ping.write();

        // If a ping is already pending, don't spawn another one
        if let Some(ping) = &mut *ping {
            if !ping.is_finished() {
                return;
            }
        }

        *ping = Some(BackgroundPing {
            request: tokio::spawn(Self::ping(
                api,
                self.config.bg_timeout,
                self.config.bg_retry,
            )),
        });
    }

    fn is_cache_fresh(&self) -> bool {
        self.cache
            .read()
            .checked_at
            .is_some_and(|at| at.elapsed() < self.config.up_to_date)
    }

    fn get_cached_status(&self) -> ConnectionStatus {
        self.cache.read().status
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
                self.on_recv_ok(&resp);
                Ok(resp)
            }

            Err(error) => {
                self.on_recv_err(&error);
                Err(error)
            }
        }
    }

    fn on_recv_err(&self, error: &MuonError) {
        use ErrorKind::*;

        match error.kind() {
            Tls | Resolve | Dial | Send | SendRetry => {
                self.update(ConnectionStatus::Offline);
            }

            Connect => {
                self.update(ConnectionStatus::ServerUnreachable);
            }

            _ => {}
        }
    }

    fn on_recv_ok(&self, resp: &ProtonResponse) {
        if resp.is(429) || resp.status().is_server_error() {
            self.update(ConnectionStatus::ServerUnreachable);
        } else {
            self.update(ConnectionStatus::Online);
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
