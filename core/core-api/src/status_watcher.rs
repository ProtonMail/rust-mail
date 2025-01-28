use std::{
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use muon::{
    common::{BoxFut, Sender, SenderLayer},
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
}

impl StatusWatcher {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let resp = inner.send(req).await?.ok();

        if let Err(error) = resp {
            self.update(ConnectionStatus::Offline).await;
            Err(error.into())
        } else {
            self.update(ConnectionStatus::Online).await;
            resp.map_err(Into::into)
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
    pub fn new() -> Self {
        Self {
            status: STATUS.clone(),
            last_check: Arc::new(RwLock::new(
                Instant::now()
                    .checked_sub(Duration::from_secs(UP_TO_DATE_SECONDS))
                    .unwrap(),
            )),
            request: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn status(&self, api: Proton) -> ConnectionStatus {
        if !self.is_up_to_date().await {
            let opt_request = self.request.lock().await.take();
            if let Some(request) = opt_request {
                if let Some(status) = request.await.ok() {
                    self.update(status).await;
                }
            } else {
                self.update(Self::ping(api.clone()).await).await;
            }
        }
        let status = *self.status.read().await;

        if status.is_offline() {
            self.background_check(api).await
        }

        status
    }

    pub async fn update(&self, status: ConnectionStatus) {
        *self.last_check.write().await = Instant::now();
        *self.status.write().await = status;
    }

    async fn ping(api: Proton) -> ConnectionStatus {
        let response = api.get_tests_ping(Some(ONE_SECOND_TIMEOUT), None).await;
        match response {
            Ok(_) => ConnectionStatus::Online,
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

    // TODO: Watch for going online
    async fn background_check(&self, api: Proton) {
        let _ = self
            .request
            .lock()
            .await
            .insert(tokio::spawn(async move { Self::ping(api).await }));
    }

    async fn is_up_to_date(&self) -> bool {
        self.last_check.read().await.elapsed().as_secs() < 1
    }
}
