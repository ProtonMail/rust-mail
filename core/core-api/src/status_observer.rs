mod fixed_queue;

use self::fixed_queue::StatusChanges;
use crate::services::proton::ProtonCore;
use crate::services::proton::common::Timeouts;
use crate::{connection_status::ConnectionStatus, services::proton::Proton};
use derive_more::Debug;
use muon::common::{BoxFut, RetryPolicy, Sender, SenderLayer};
use muon::error::ErrorKind;
use muon::util::DurationExt;
use muon::{Error as MuonError, ProtonRequest, ProtonResponse, Result as MuonResult};
use parking_lot::RwLock;
use proton_task_service::{DynSpawner, SpawnerRef};
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{instrument, trace, warn};

const UP_TO_DATE_DURATION: Duration = Duration::from_secs(6);
const LOW_LATENCY_UP_TO_DATE_DURATION: Duration = Duration::from_secs(1);

static CACHE: LazyLock<Arc<RwLock<CachedStatus>>> = LazyLock::new(|| {
    Arc::new(RwLock::new(CachedStatus {
        status: ConnectionStatus::Online,
        checked_at: None,
    }))
});

#[must_use]
#[derive(Clone, Debug)]
pub struct StatusObserver {
    cache: Arc<RwLock<CachedStatus>>,
    config: StatusObserverConfig,
    status: watch::Sender<ConnectionStatus>,
    history: StatusChanges,

    #[debug(skip)]
    ping: Arc<RwLock<Option<BackgroundPing>>>,

    #[debug(skip)]
    spawner: SpawnerRef,
}

impl StatusObserver {
    pub fn new(spawner: SpawnerRef) -> Self {
        let (status, _) = watch::channel(ConnectionStatus::Online);

        Self {
            cache: Arc::clone(&CACHE),
            config: StatusObserverConfig::default(),
            status,
            history: StatusChanges::new(NonZeroUsize::new(3).unwrap()),
            ping: Arc::new(RwLock::new(None)),
            spawner,
        }
    }

    #[cfg(feature = "mocks")]
    pub fn test(spawner: SpawnerRef) -> Self {
        let (status, _) = watch::channel(ConnectionStatus::Online);
        let config = StatusObserverConfig::test();

        Self {
            cache: Arc::new(RwLock::new(CachedStatus {
                status: ConnectionStatus::Online,
                checked_at: None,
            })),
            config,
            status,
            history: StatusChanges::new(NonZeroUsize::new(3).unwrap()),
            ping: Arc::new(RwLock::new(None)),
            spawner,
        }
    }

    #[cfg(feature = "mocks")]
    pub fn set_up_to_date(&mut self, up_to_date: Duration) {
        let stale_instant = Instant::now()
            .checked_sub(Duration::from_secs(up_to_date.as_secs() + 1))
            .unwrap();

        self.cache.write().checked_at = Some(stale_instant);
        self.config.up_to_date = up_to_date;
    }

    #[instrument(skip_all)]
    pub async fn status(&self, api: Proton) -> ConnectionStatus {
        self.status_fresher_than(api, self.config.up_to_date, false)
            .await
    }

    /// Get the current status of the connection with low latency.
    ///
    /// Method is equivalent to [`status`] but with a distinction of
    /// making sure the fresh time is very short and it forces
    /// running low timeout ping request in foreground to establish
    /// almost real time of the current status.
    ///
    /// This method should be used only in very specific environments
    /// which would rather fallback to serve cached data over online
    /// when the online status is uncertain due to the fact it may
    /// impair usage of the application.
    ///
    /// Due to the fact that this method is fast reacting it may produce a lot
    /// of network trafic, so if you are uncertain which method to use, default
    /// to [`status`] method instead.
    ///
    #[instrument(skip_all)]
    pub async fn low_latency_status(&self, api: Proton) -> ConnectionStatus {
        self.status_fresher_than(api, LOW_LATENCY_UP_TO_DATE_DURATION, true)
            .await
    }

    #[instrument(skip_all)]
    async fn status_fresher_than(
        &self,
        api: Proton,
        than: Duration,
        force_update: bool,
    ) -> ConnectionStatus {
        if !self.is_cache_fresher_than(than) {
            if force_update
                || (self.get_cached_status().is_offline()
                    && self.history.was_online_most_of_the_time())
            {
                self.clone()
                    .ping(api.clone(), self.config.fg_timeout, self.config.fg_retry)
                    .await;
            } else {
                self.spawn_ping(api.clone());
            }
        }

        // We want to spawn background ping
        // anytime we are offline to get
        // online notification as soon as possible
        if self.get_cached_status().is_offline() {
            self.spawn_ping(api);
        }

        self.get_cached_status()
    }

    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<ConnectionStatus> {
        self.status.subscribe()
    }

    #[instrument(skip_all)]
    fn update(&self, new: ConnectionStatus) {
        let mut cache = self.cache.write();

        self.status.send_if_modified(|old| {
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

    #[instrument(skip_all)]
    async fn ping(self, api: Proton, timeout: Duration, retry: RetryPolicy) {
        let new_status = async {
            let status = StatusObserver::do_ping(api.clone(), timeout).await;
            if status.is_online() {
                return status;
            }

            let mut last_status = status;
            for timeout in retry {
                tokio::time::sleep(timeout).await;
                last_status = Self::do_ping(api.clone(), timeout).await;
                if last_status.is_online() {
                    return last_status;
                }
            }
            last_status
        }
        .await;

        self.update(new_status);
    }
    async fn do_ping(api: Proton, timeout: Duration) -> ConnectionStatus {
        match api
            .get_tests_ping(Some(timeout), Some(RetryPolicy::default().never()))
            .await
        {
            Err(e) if e.is_server_unreachable() => ConnectionStatus::ServerUnreachable,
            Err(e) if e.is_network_failure() => ConnectionStatus::Offline,
            _ => ConnectionStatus::Online,
        }
    }

    #[instrument(skip_all)]
    fn spawn_ping(&self, api: Proton) {
        let mut ping = self.ping.write();

        if let Some(ping) = &mut *ping {
            if !ping.is_finished() {
                return;
            }
        }

        let this = self.clone();

        *ping = Some(BackgroundPing {
            request: self.spawner.spawn_boxed_task(Box::pin(this.ping(
                api,
                self.config.bg_timeout,
                self.config.bg_retry,
            ))),
        });
    }

    fn is_cache_fresher_than(&self, than: Duration) -> bool {
        self.cache
            .read()
            .checked_at
            .is_some_and(|at| at.elapsed() < than)
    }

    fn get_cached_status(&self) -> ConnectionStatus {
        self.cache.read().status
    }
}

#[derive(Clone, Debug)]
struct StatusObserverConfig {
    fg_retry: RetryPolicy,
    fg_timeout: Duration,
    bg_retry: RetryPolicy,
    bg_timeout: Duration,
    up_to_date: Duration,
}

impl StatusObserverConfig {
    fn new() -> Self {
        Self {
            up_to_date: UP_TO_DATE_DURATION,
            fg_retry: RetryPolicy::default().never(),
            fg_timeout: Timeouts::TWO_SECONDS,
            bg_retry: RetryPolicy::default()
                .max_count(2)
                .max_delay(5.s())
                .iter_mul(1.0),
            bg_timeout: Timeouts::QUARTER_MINUTE,
        }
    }

    #[cfg(feature = "mocks")]
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

impl Default for StatusObserverConfig {
    fn default() -> Self {
        Self::new()
    }
}

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

struct BackgroundPing {
    request: JoinHandle<()>,
}

impl BackgroundPing {
    fn is_finished(&self) -> bool {
        self.request.is_finished()
    }
}

#[derive(Debug)]
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
            Tls | Resolve | Dial | Send => {
                self.0.update(ConnectionStatus::Offline);
            }

            Connect => {
                self.0.update(ConnectionStatus::ServerUnreachable);
            }

            _ => {}
        }
    }

    fn on_recv_ok(&self, resp: &ProtonResponse) {
        if resp.is(429) || resp.status().is_server_error() {
            self.0.update(ConnectionStatus::ServerUnreachable);
        } else {
            self.0.update(ConnectionStatus::Online);
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
