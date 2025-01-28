use std::{
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

use crate::{
    connection_status::ConnectionStatus,
    services::proton::{Proton, ProtonCore, ONE_SECOND_TIMEOUT},
};

type StatusJoinHandle = JoinHandle<ConnectionStatus>;

const UP_TO_DATE_SECONDS: u64 = 5;
static STATUS: LazyLock<Arc<RwLock<ConnectionStatus>>> =
    LazyLock::new(|| Arc::new(RwLock::new(ConnectionStatus::Online)));

#[derive(Clone, Debug)]
pub struct StatusWatcher {
    status: Arc<RwLock<ConnectionStatus>>,
    last_check: Arc<RwLock<Instant>>,
    request: Arc<Mutex<Option<StatusJoinHandle>>>,
    last_request: Arc<RwLock<Instant>>,
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
                    ErrorKind::Tls | ErrorKind::Resolve | ErrorKind::Dial | ErrorKind::Connect => {
                        self.update(ConnectionStatus::Offline).await;
                    }
                    ErrorKind::Send | ErrorKind::Closed => {
                        self.update(ConnectionStatus::ServerUnreachable).await;
                    }
                    _ => {}
                }

                Err(error)
            }
            Ok(resp) => {
                let status = resp.status();
                if status.is_client_error() {
                    self.update(ConnectionStatus::Offline).await;
                } else if status.is_server_error() {
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
            last_check: Arc::new(RwLock::new(
                Instant::now()
                    .checked_sub(Duration::from_secs(UP_TO_DATE_SECONDS))
                    .unwrap(),
            )),
            request: Arc::new(Mutex::new(None)),
            last_request: Arc::new(RwLock::new(Instant::now())),
        }
    }

    #[cfg(any(test, debug_assertions))]
    #[must_use]
    pub fn test() -> Self {
        Self {
            status: Arc::new(RwLock::new(ConnectionStatus::Online)),
            last_check: Arc::new(RwLock::new(Instant::now())),
            request: Arc::new(Mutex::new(None)),
            last_request: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Get the current status of the connection.
    /// If the status is stale, it will ping the server to get the current status.
    /// If the status is `Offline`, it will start a background check.
    ///
    pub async fn status(&self, api: Proton) -> ConnectionStatus {
        if !self.is_up_to_date().await && self.is_request_up_to_date().await {
            let opt_request = self.request.lock().await.take();
            if let Some(request) = opt_request {
                if let Ok(status) = request.await {
                    self.update(status).await;
                } else {
                    tracing::error!("Error while joining a future on status watcher. This is most likely a bug.");
                }
            } else {
                self.update(Self::ping(api.clone()).await).await;
            }
        } else {
            self.update(Self::ping(api.clone()).await).await;
        }

        let status = *self.status.read().await;

        if status.is_offline() {
            self.background_check(api).await;
        }

        status
    }

    async fn update(&self, status: ConnectionStatus) {
        *self.last_check.write().await = Instant::now();
        *self.status.write().await = status;
    }

    async fn ping(api: Proton) -> ConnectionStatus {
        let response = api.get_tests_ping(Some(ONE_SECOND_TIMEOUT), None).await;
        match response {
            Ok(()) => ConnectionStatus::Online,
            Err(error) => {
                if error.is_server_unreachable() {
                    ConnectionStatus::ServerUnreachable
                } else if error.is_network_failure() {
                    ConnectionStatus::Offline
                } else {
                    tracing::error!(
                        "Error while pinging the server: {error}. This is most likely a bug."
                    );
                    ConnectionStatus::Online
                }
            }
        }
    }

    #[allow(clippy::let_underscore_future)]
    async fn background_check(&self, api: Proton) {
        let mut request = self.request.lock().await;
        if request.is_none() {
            *self.last_request.write().await = Instant::now();
            let _ = request.insert(tokio::spawn(async move { Self::ping(api).await }));
        }
    }

    async fn is_up_to_date(&self) -> bool {
        self.last_check.read().await.elapsed().as_secs() < UP_TO_DATE_SECONDS
    }

    async fn is_request_up_to_date(&self) -> bool {
        self.last_request.read().await.elapsed().as_secs() < UP_TO_DATE_SECONDS
    }
}

impl Default for StatusWatcher {
    fn default() -> Self {
        Self::new()
    }
}
