use std::{
    ops::Deref,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use muon::{
    common::{BoxFut, Sender, SenderLayer},
    error::ErrorKind,
    ProtonRequest, ProtonResponse, Result as MuonResult,
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
};
use tracing::trace;

use crate::{
    connection_status::ConnectionStatus,
    services::proton::{Proton, ProtonCore, ONE_MINUTE_TIMEOUT, ONE_SECOND_TIMEOUT},
};

type StatusJoinHandle = JoinHandle<()>;

const UP_TO_DATE_SECONDS: u64 = 5;
static STATUS: LazyLock<Arc<RwLock<Status>>> = LazyLock::new(|| {
    Arc::new(RwLock::new(Status {
        status: ConnectionStatus::Online,
        last_check: Instant::now()
            .checked_sub(Duration::from_secs(UP_TO_DATE_SECONDS + 1))
            .unwrap(),
    }))
});

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

#[derive(Debug)]
struct BackgroundPing {
    _request: StatusJoinHandle,
    finished: flume::Receiver<()>,
}

#[derive(Clone, Debug)]
pub struct StatusWatcher {
    status: Arc<RwLock<Status>>,
    request: Arc<Mutex<Option<BackgroundPing>>>,
    up_to_date: Duration,
}

impl StatusWatcher {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let resp = inner.send(req).await;

        match resp {
            Err(error) => {
                match error.kind() {
                    ErrorKind::Tls | ErrorKind::Resolve | ErrorKind::Dial | ErrorKind::Send => {
                        self.update(ConnectionStatus::Offline).await;
                    }
                    ErrorKind::Connect | ErrorKind::Closed => {
                        self.update(ConnectionStatus::ServerUnreachable).await;
                    }
                    _ => {}
                }

                Err(error)
            }
            Ok(resp) => {
                if resp.is(429) || resp.status().is_server_error() {
                    self.update(ConnectionStatus::ServerUnreachable).await;
                } else {
                    self.update(ConnectionStatus::Online).await;
                }

                Ok(resp)
            }
        }
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for StatusWatcher {
    fn on_send<'a: 'fut, 'fut>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'fut, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

impl StatusWatcher {
    /// Create a new `StatusWatcher`.
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
        Self {
            status: STATUS.clone(),
            request: Arc::new(Mutex::new(None)),
            up_to_date: Duration::from_secs(UP_TO_DATE_SECONDS),
        }
    }
    /// Create a new test `StatusWatcher` without shared state.
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
    pub fn test() -> Self {
        let stale_instant = Instant::now()
            .checked_sub(Duration::from_secs(UP_TO_DATE_SECONDS + 1))
            .unwrap();
        Self {
            status: Arc::new(RwLock::new(Status {
                status: ConnectionStatus::Online,
                last_check: stale_instant,
            })),
            request: Arc::new(Mutex::new(None)),
            up_to_date: Duration::from_secs(UP_TO_DATE_SECONDS),
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
    pub async fn with_up_to_date(self, up_to_date: Duration) -> Self {
        let stale_instant = Instant::now()
            .checked_sub(Duration::from_secs(up_to_date.as_secs() + 1))
            .unwrap();
        self.status.write().await.last_check = stale_instant;
        Self { up_to_date, ..self }
    }

    /// Get the current status of the connection.
    /// If the status is stale, it will ping the server to get the current status.
    /// If the status is `Offline`, it will start a background check.
    ///
    pub async fn status(&self, api: Proton) -> ConnectionStatus {
        if !self.is_up_to_date().await {
            let mut request = self.request.lock().await;
            if let Some(request_data) = request.as_ref() {
                if !request_data.finished.is_empty()
                    && request_data.finished.recv_async().await.is_ok()
                {
                    drop(request.take());
                }
            }
            drop(request);

            Self::ping(api.clone(), ONE_SECOND_TIMEOUT).await;
        }

        let status = self.status.read().await;

        if status.is_offline() {
            self.background_check(api).await;
        }

        status.status
    }

    async fn update(&self, status: ConnectionStatus) {
        let mut self_status = self.status.write().await;
        self_status.last_check = Instant::now();
        self_status.status = status;

        trace!("Status has been updated to {:?}", status);
    }

    async fn ping(api: Proton, timeout: u64) {
        let _ = api.get_tests_ping(Some(timeout), None).await;
    }

    #[allow(clippy::let_underscore_future)]
    async fn background_check(&self, api: Proton) {
        let mut request = self.request.lock().await;
        if request.is_none() {
            let (sender, receiver) = flume::unbounded();
            let _ = request.insert(BackgroundPing {
                _request: tokio::spawn(async move {
                    Self::ping(api, ONE_MINUTE_TIMEOUT).await;
                    let _ = sender.send_async(()).await;
                }),
                finished: receiver,
            });
        }
    }

    async fn is_up_to_date(&self) -> bool {
        self.status.read().await.last_check.elapsed() < self.up_to_date
    }
}

impl Default for StatusWatcher {
    fn default() -> Self {
        Self::new()
    }
}
