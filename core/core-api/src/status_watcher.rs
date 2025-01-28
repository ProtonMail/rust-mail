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

type StatusJoinHandle = JoinHandle<()>;

const UP_TO_DATE_SECONDS: u64 = 5;
static STATUS: LazyLock<Arc<RwLock<ConnectionStatus>>> =
    LazyLock::new(|| Arc::new(RwLock::new(ConnectionStatus::Online)));

#[derive(Clone, Debug)]
pub struct StatusWatcher {
    status: Arc<RwLock<ConnectionStatus>>,
    last_check: Arc<RwLock<Instant>>,
    request: Arc<Mutex<Option<StatusJoinHandle>>>,
}

impl StatusWatcher {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        dbg!(&req);
        let resp = inner.send(req).await;

        match resp {
            Err(error) => {
                dbg!(&error);
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
                dbg!(&resp);
                if resp.is(404) || resp.is(429) || resp.status().is_server_error() {
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
        let stale_instant = Instant::now()
            .checked_sub(Duration::from_secs(UP_TO_DATE_SECONDS + 1))
            .unwrap();
        Self {
            status: STATUS.clone(),
            last_check: Arc::new(RwLock::new(stale_instant)),
            request: Arc::new(Mutex::new(None)),
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
            status: Arc::new(RwLock::new(ConnectionStatus::Online)),
            last_check: Arc::new(RwLock::new(stale_instant)),
            request: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the current status of the connection.
    /// If the status is stale, it will ping the server to get the current status.
    /// If the status is `Offline`, it will start a background check.
    ///
    pub async fn status(&self, api: Proton) -> ConnectionStatus {
        if !self.is_up_to_date().await {
            drop(self.request.lock().await.take());
            Self::ping(api.clone()).await;
        }

        let status = *self.status.read().await;

        if status.is_offline() {
            self.background_check(api).await;
        }

        status
    }

    async fn update(&self, status: ConnectionStatus) {
        dbg!("UPDATE STATUS");
        *self.last_check.write().await = Instant::now();
        *self.status.write().await = status;
    }

    async fn ping(api: Proton) {
        let _ = api.get_tests_ping(Some(ONE_SECOND_TIMEOUT), None).await;
    }

    #[allow(clippy::let_underscore_future)]
    async fn background_check(&self, api: Proton) {
        let mut request = self.request.lock().await;
        if request.is_none() {
            let _ = request.insert(tokio::spawn(async move { Self::ping(api).await }));
        }
    }

    async fn is_up_to_date(&self) -> bool {
        dbg!("IS UP TO DATE");
        dbg!(self.last_check.read().await.elapsed().as_secs() < UP_TO_DATE_SECONDS)
    }
}

impl Default for StatusWatcher {
    fn default() -> Self {
        Self::new()
    }
}
