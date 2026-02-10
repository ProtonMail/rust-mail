use crate::{
    ConnectionMonitor, NetworkStatusObserver, OnlineTester, OsNetworkStatus,
    OsNetworkStatusObserver, RequestNetworkStatus, update_watcher_value,
};
use futures::FutureExt;
use muon::common::RetryPolicy;
use proton_task_service::{DynSpawner, SpawnerRef};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::instrument;

#[derive(Default, Debug, Clone)]
pub struct Config {
    pub immediate: ImmediateConfig,
    pub background: BackgroundConfig,
}

#[derive(Debug, Clone)]
pub struct ImmediateConfig {
    /// Duration of time one is willing to wait for the result of the immediate request.
    /// The request will continue in the background, but will return cached status if we time
    /// out.
    pub command_timeout: Duration,
    pub request_timeout: Duration,
    pub retry_policy: RetryPolicy,
    /// The amount of time to wait before another quick check can be made.
    pub retry_interval: Duration,
}

impl Default for ImmediateConfig {
    fn default() -> Self {
        Self {
            command_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(20),
            retry_policy: RetryPolicy::default(),
            retry_interval: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackgroundConfig {
    pub timeout: Duration,
    pub retry_policy: RetryPolicy,
    /// When this is set, the background task tries forever, otherwise we only try
    /// up to `max_count` from the retry policy
    pub infinite_checks: bool,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            retry_policy: RetryPolicy::default()
                .max_delay(Duration::from_secs(30))
                .max_count(usize::MAX)
                .iter_add(Duration::from_secs(1))
                .iter_mul(1.5),
            infinite_checks: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkMonitorServiceError {
    #[error("Monitor has already started")]
    AlreadyStarted,
}

/// The network monitor service combines both the OS network info feedback with connection
/// monitors to provide an "are we connected" state.
///
/// Background checks will be slower, if more immediate feedback is required, and immediate
/// check can be performed to ascertain the status of the network at that given point in time.
pub struct NetworkMonitorService {
    os_network_watcher: watch::Sender<OsNetworkStatus>,
    request_network_watcher: watch::Sender<RequestNetworkStatus>,
    subscriber_watcher: watch::Sender<RequestNetworkStatus>,
    monitor_task: Option<JoinHandle<()>>,
    config: Config,
    immediate_test_requester: Option<mpsc::Sender<oneshot::Sender<RequestNetworkStatus>>>,
}

impl NetworkMonitorService {
    #[must_use]
    pub fn new(config: Config) -> Self {
        let (os_sender, _) = watch::channel(OsNetworkStatus::Online);
        let (request_sender, _) = watch::channel(RequestNetworkStatus::Online);
        let (subscriber_sender, _) = watch::channel(RequestNetworkStatus::Online);
        Self {
            os_network_watcher: os_sender,
            request_network_watcher: request_sender,
            subscriber_watcher: subscriber_sender,
            monitor_task: None,
            config,
            immediate_test_requester: None,
        }
    }

    pub fn update_os_network_status(&self, status: OsNetworkStatus) {
        update_watcher_value(&self.os_network_watcher, status);
    }

    #[must_use]
    pub fn new_connection_monitor(&self) -> ConnectionMonitor {
        ConnectionMonitor::monitored(self)
    }

    pub fn start(
        &mut self,
        spawner: &SpawnerRef,
        tester: Arc<dyn OnlineTester>,
    ) -> Result<(), NetworkMonitorServiceError> {
        if self.monitor_task.is_some() {
            return Err(NetworkMonitorServiceError::AlreadyStarted);
        }

        let (sender, receiver) = mpsc::channel(1);
        let task = NetworkMonitorBackgroundTask::new(self, spawner.clone(), tester, receiver);
        self.immediate_test_requester = Some(sender);
        self.monitor_task = Some(tokio::spawn(async move { task.run().await }.boxed()));
        Ok(())
    }

    pub(crate) fn request_watcher(&self) -> watch::Sender<RequestNetworkStatus> {
        self.request_network_watcher.clone()
    }

    pub(crate) fn subscriber_watcher(&self) -> watch::Sender<RequestNetworkStatus> {
        self.subscriber_watcher.clone()
    }

    #[must_use]
    pub fn network_status_observer(&self) -> NetworkStatusObserver {
        NetworkStatusObserver::new(self.subscriber_watcher.subscribe())
    }

    #[must_use]
    pub fn os_network_status_observer(&self) -> OsNetworkStatusObserver {
        OsNetworkStatusObserver::new(self.os_network_watcher.subscribe())
    }

    #[must_use]
    pub fn is_online(&self) -> bool {
        self.network_status_observer().is_online()
    }

    #[must_use]
    pub fn is_os_online(&self) -> bool {
        self.os_network_status_observer().is_online()
    }

    /// This method will perform an network test immediately.
    ///
    /// If we don't get a response before `ImmediateConfig.command_timeout`, we will return
    /// a cached status.
    ///
    /// If this request happens less `ImmediateConfig.retry_interval` before another request, the
    /// cached status will be returned.
    pub async fn check_now(&self) -> RequestNetworkStatus {
        self.check_now_deferred().await
    }

    // This is a compatability method to work around the fact that the service initializer
    // in core-common does not have mut access.
    pub fn check_now_deferred(&self) -> impl Future<Output = RequestNetworkStatus> + 'static {
        let requester = self.immediate_test_requester.clone();
        let network_status_observer = self.network_status_observer();
        let command_timeout = self.config.immediate.command_timeout;
        async move {
            let Some(sender) = requester else {
                tracing::warn!(
                    "Service has not started, immediate requests will respond with online"
                );
                return RequestNetworkStatus::Online;
            };
            let (oneshot_sender, oneshot_receiver) = oneshot::channel();
            if sender.send(oneshot_sender).await.is_err() {
                tracing::warn!("Failed to communicate with the network monitor service");
                return RequestNetworkStatus::Online;
            }

            match tokio::time::timeout(command_timeout, oneshot_receiver).await {
                Err(_) => {
                    // Timed out, return current value
                    network_status_observer.status()
                }
                Ok(Ok(v)) => v,
                Ok(Err(_)) => {
                    tracing::warn!(
                        "Failed to communicate with the network monitor service, returning last status"
                    );
                    network_status_observer.status()
                }
            }
        }
    }
}

impl Drop for NetworkMonitorService {
    fn drop(&mut self) {
        if let Some(handle) = self.monitor_task.take() {
            handle.abort();
        }
    }
}

struct NetworkMonitorBackgroundTask {
    config: Config,
    os_network_subscriber: watch::Receiver<OsNetworkStatus>,
    request_network_subscriber: watch::Receiver<RequestNetworkStatus>,
    request_watcher: watch::Sender<RequestNetworkStatus>,
    subscriber_watcher: watch::Sender<RequestNetworkStatus>,
    tester: Arc<dyn OnlineTester>,
    os_network_status: OsNetworkStatus,
    request_network_status: RequestNetworkStatus,
    subscriber_status: RequestNetworkStatus,
    spawner: SpawnerRef,
    tester_task: Option<JoinHandle<()>>,
    immediate_request: mpsc::Receiver<oneshot::Sender<RequestNetworkStatus>>,
    last_immediate_check: Instant,
    immediate_check_running: bool,
    immediate_check_result_receiver: mpsc::Receiver<RequestNetworkStatus>,
    immediate_check_request_sender: mpsc::Sender<oneshot::Sender<RequestNetworkStatus>>,
}

impl NetworkMonitorBackgroundTask {
    fn new(
        monitor: &NetworkMonitorService,
        spawner: SpawnerRef,
        tester: Arc<dyn OnlineTester>,
        immediate_request: mpsc::Receiver<oneshot::Sender<RequestNetworkStatus>>,
    ) -> Self {
        let os_network_subscriber = monitor.os_network_watcher.subscribe();
        let request_network_subscriber = monitor.request_network_watcher.subscribe();
        let subscriber_watcher = monitor.subscriber_watcher.clone();
        let os_network_status = *os_network_subscriber.borrow();
        let request_network_status = *request_network_subscriber.borrow();
        let config = monitor.config.clone();
        let mut last_immediate_check = Instant::now();
        if let Some(new_value) = last_immediate_check.checked_sub(config.immediate.retry_interval) {
            last_immediate_check = new_value;
        }

        let (immediate_check_request_sender, immediate_check_request_receiver) = mpsc::channel(1);
        let (immediate_check_result_sender, immediate_check_result_receiver) = mpsc::channel(1);

        let immediate_config = config.immediate.clone();
        let tester_cloned = tester.clone();

        let request_watcher_cloned = monitor.request_watcher();
        spawner.spawn_boxed_task(
            async move {
                immediate_tester(
                    immediate_config,
                    tester_cloned.as_ref(),
                    immediate_check_request_receiver,
                    immediate_check_result_sender,
                    request_watcher_cloned,
                )
                .await;
            }
            .boxed(),
        );

        let mut instance = Self {
            config,
            os_network_subscriber,
            request_network_subscriber,
            subscriber_watcher,
            tester,
            os_network_status,
            request_network_status,
            subscriber_status: RequestNetworkStatus::Online,
            spawner,
            tester_task: None,
            immediate_request,
            request_watcher: monitor.request_watcher(),
            last_immediate_check,
            immediate_check_running: false,
            immediate_check_request_sender,
            immediate_check_result_receiver,
        };
        tracing::debug!(
            "Current status os={os_network_status:?} request={request_network_status:?}"
        );
        instance.update_subscriber_status();

        instance
    }
    #[instrument(skip_all, name = "network_monitor_service")]
    async fn run(mut self) {
        tracing::info!("Starting NetworkMonitorService");

        loop {
            tokio::select! {
                _ = self.os_network_subscriber.changed() => {
                    self.update_os_network_status().await;
                }
                _= self.request_network_subscriber.changed() => {
                    self.update_request_network_status();
                }
                v = self.immediate_check_result_receiver.recv() => {
                    if v.is_some() {
                        tracing::debug!("Immediate check completed");
                        self.immediate_check_running=false;
                    }
                }
                r = self.immediate_request.recv() => {
                    if let Some(sender)  = r {
                        self.handle_immediate_request(sender).await;
                    }
                }
            }
        }
    }

    async fn update_os_network_status(&mut self) {
        let new_os_network_status = *self.os_network_subscriber.borrow();
        if new_os_network_status != self.os_network_status {
            self.os_network_status = new_os_network_status;
            tracing::debug!("OS Network status updated: {:?}", self.os_network_status);
            if self.os_network_status == OsNetworkStatus::Online {
                // Perform one check to validate we are actually online
                // and restart the ping task if necessary
                let network_status = self
                    .tester
                    .check(self.config.immediate.request_timeout)
                    .await;
                update_watcher_value(&self.request_watcher, network_status);
                self.request_network_status = network_status;
            } else if let Some(task) = self.tester_task.take() {
                tracing::debug!("Terminating ping task from os network loss update");
                task.abort();
            }
            self.update_subscriber_status();
        }
    }

    fn update_request_network_status(&mut self) {
        let new_request_network_status = *self.request_network_subscriber.borrow();
        if new_request_network_status != self.request_network_status {
            self.request_network_status = new_request_network_status;
            tracing::debug!(
                "Network request status updated: {:?}",
                self.request_network_status
            );
            self.update_subscriber_status();
        }
    }

    fn update_subscriber_status(&mut self) {
        let subscriber_status =
            RequestNetworkStatus::combine(self.os_network_status, self.request_network_status);
        update_watcher_value(&self.subscriber_watcher, subscriber_status);
        if self.subscriber_status != subscriber_status {
            if subscriber_status.is_online() {
                tracing::info!("Network is back online");
                if let Some(task) = self.tester_task.take() {
                    tracing::debug!("Cancelling tester task");
                    task.abort();
                }
            } else {
                tracing::info!("Network connection lost");
                if self.os_network_status == OsNetworkStatus::Online {
                    if self.tester_task.is_none() {
                        tracing::debug!("Starting tester task");
                        let config = self.config.background.clone();
                        let tester = self.tester.clone();
                        let watcher = self.request_watcher.clone();
                        self.tester_task = Some(
                            self.spawner.spawn_boxed_task(
                                async move {
                                    network_tester(config, tester.as_ref(), watcher).await;
                                }
                                .boxed(),
                            ),
                        );
                    }
                } else if let Some(task) = self.tester_task.take() {
                    tracing::debug!("Cancelling tester task due to offline os update");
                    task.abort();
                }
            }
            tracing::info!("Subscriber status changed to {:?}", subscriber_status);
            self.subscriber_status = subscriber_status;
        }
    }

    async fn handle_immediate_request(&mut self, sender: oneshot::Sender<RequestNetworkStatus>) {
        if !self.immediate_check_running
            && self.last_immediate_check.elapsed() > self.config.immediate.retry_interval
        {
            self.last_immediate_check = Instant::now();
            self.immediate_check_running = true;
            if let Err(mpsc::error::SendError(sender)) =
                self.immediate_check_request_sender.send(sender).await
            {
                tracing::warn!("immediate tester is dead, returning cached value");
                let _ = sender.send(self.request_network_status);
            }
        } else {
            if self.immediate_check_running {
                tracing::debug!(
                    "Received immediate check request, but last request is still ongoing. Using cached value"
                );
            } else {
                tracing::debug!(
                    "Received immediate check request, but still too soon. Using cached value"
                );
            }
            let _ = sender.send(self.request_network_status);
        }
    }
}

#[instrument(skip_all, name = "network_monitor_tester")]
async fn network_tester(
    config: BackgroundConfig,
    tester: &dyn OnlineTester,
    watcher: watch::Sender<RequestNetworkStatus>,
) {
    tracing::info!("Starting...");
    loop {
        let status =
            perform_tester_check(tester, config.timeout, config.retry_policy, &watcher).await;
        if status.is_online() {
            tracing::info!("Network detected as online, exiting");
            return;
        }

        if !config.infinite_checks {
            tracing::info!("Network exiting, max checks reached");
            return;
        }
    }
}

#[instrument(skip_all, name = "network_monitor_immediate_test")]
async fn immediate_tester(
    config: ImmediateConfig,
    tester: &dyn OnlineTester,
    mut receiver: mpsc::Receiver<oneshot::Sender<RequestNetworkStatus>>,
    main_sender: mpsc::Sender<RequestNetworkStatus>,
    request_watcher: watch::Sender<RequestNetworkStatus>,
) {
    tracing::info!("Starting");
    loop {
        while let Some(sender) = receiver.recv().await {
            tracing::debug!("Performing immediate check...");
            let status = perform_tester_check(
                tester,
                config.request_timeout,
                config.retry_policy,
                &request_watcher,
            )
            .await;
            tracing::debug!("Performing immediate check... -> {status:?}");

            let _ = sender.send(status);

            if main_sender.send(status).await.is_err() {
                return;
            }
        }
    }
}

async fn perform_tester_check(
    tester: &dyn OnlineTester,
    timeout: Duration,
    retry_policy: RetryPolicy,
    request_watcher: &watch::Sender<RequestNetworkStatus>,
) -> RequestNetworkStatus {
    let mut status = tester.check(timeout).await;
    update_watcher_value(request_watcher, status);
    if status.is_online() {
        return status;
    }

    for duration in retry_policy {
        tracing::debug!("Network is still not online, waiting for {duration:?} before retrying");
        tokio::time::sleep(duration).await;
        status = tester.check(timeout).await;
        update_watcher_value(request_watcher, status);
        if status.is_online() {
            break;
        }
    }

    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockOnlineTester;
    use mockall::Sequence;
    use mockall::predicate::eq;
    use std::sync::RwLock;
    use tracing::subscriber::set_global_default;
    use tracing_subscriber::fmt::layer;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{EnvFilter, registry};

    #[tokio::test(flavor = "multi_thread")]
    async fn os_updates() {
        let mut tester = MockOnlineTester::new();
        tester
            .expect_check()
            .returning(|_| RequestNetworkStatus::Online);

        let service = new_service(test_config(), tester);
        let mut observer = service.network_status_observer();

        assert_eq!(observer.status(), RequestNetworkStatus::Online);
        service.update_os_network_status(OsNetworkStatus::Offline);

        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::Offline);
        // same status update is noop
        service.update_os_network_status(OsNetworkStatus::Offline);
        tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap_err();

        service.update_os_network_status(OsNetworkStatus::Online);
        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::Online);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn os_updates_resume_network_ping() {
        let mut config = test_config();
        config.immediate.request_timeout = Duration::from_millis(5);
        config.background.retry_policy = RetryPolicy::default()
            .min_delay(Duration::from_secs(60))
            .max_delay(Duration::from_secs(60))
            .jitter(Duration::from_secs(0))
            .iter_mul(1.0)
            .iter_add(Duration::from_secs(0));

        let mut tester = MockOnlineTester::new();
        tester
            .expect_check()
            .with(eq(config.background.timeout))
            .times(1..=2)
            .returning(|_| RequestNetworkStatus::Offline);

        tester
            .expect_check()
            .once()
            .with(eq(config.immediate.request_timeout))
            .returning(|_| RequestNetworkStatus::ServerUnreachable);
        let service = new_service(config, tester);

        let mut observer = service.network_status_observer();

        // Report one connection with a server unreachable
        service
            .new_connection_monitor()
            .update_request_status(RequestNetworkStatus::ServerUnreachable);
        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::ServerUnreachable);

        // Report server as offline - will cancel the current background task
        service.update_os_network_status(OsNetworkStatus::Offline);
        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::Offline);

        // Report os online - will trigger the immediate check that will bring the service back to server unreachable
        service.update_os_network_status(OsNetworkStatus::Online);
        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::ServerUnreachable);

        // The next update will come from the background task which will report the service as offline.
        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::Offline);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn request_updates() {
        let value_to_report = Arc::new(RwLock::new(RequestNetworkStatus::Offline));
        let mut tester = MockOnlineTester::new();
        let value_to_report_cloned = value_to_report.clone();
        tester
            .expect_check()
            .returning(move |_| *value_to_report_cloned.read().unwrap());

        let service = new_service(test_config(), tester);
        let mut observer = service.network_status_observer();

        // force into offline to start the monitor
        service
            .new_connection_monitor()
            .update_request_status(RequestNetworkStatus::Offline);

        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::Offline);
        *value_to_report.write().unwrap() = RequestNetworkStatus::ServerUnreachable;
        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::ServerUnreachable);
        *value_to_report.write().unwrap() = RequestNetworkStatus::Online;
        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::Online);

        // Even if the request is online, when os reports offline it should take priority
        service.update_os_network_status(OsNetworkStatus::Offline);
        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::Offline);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn immediate_request_throttling() {
        let mut tester = MockOnlineTester::new();
        let mut config = test_config();
        config.background.retry_policy.min_delay = Duration::from_secs(50);
        tester
            .expect_check()
            .once()
            .with(eq(config.background.timeout))
            .returning(move |_| RequestNetworkStatus::Offline);
        let mut sequence = Sequence::new();
        tester
            .expect_check()
            .once()
            .in_sequence(&mut sequence)
            .with(eq(config.immediate.request_timeout))
            .returning(|_| RequestNetworkStatus::Offline);
        tester
            .expect_check()
            .once()
            .in_sequence(&mut sequence)
            .with(eq(config.immediate.request_timeout))
            .returning(|_| RequestNetworkStatus::Online);

        let service = new_service(config.clone(), tester);
        let mut observer = service.network_status_observer();

        // force into offline to start the monitor
        service
            .new_connection_monitor()
            .update_request_status(RequestNetworkStatus::Offline);

        let new_status = tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
            .await
            .unwrap();
        assert_eq!(new_status, RequestNetworkStatus::Offline);

        let status = service.check_now().await;
        assert_eq!(status, RequestNetworkStatus::Offline);
        let status = service.check_now().await;
        assert_eq!(status, RequestNetworkStatus::Offline);
        tokio::time::sleep(config.immediate.retry_interval).await;
        let status = service.check_now().await;
        assert_eq!(status, RequestNetworkStatus::Online);

        let observer_status =
            tokio::time::timeout(Duration::from_secs(1), observer.wait_for_change())
                .await
                .unwrap();
        assert_eq!(observer_status, RequestNetworkStatus::Online);
    }

    fn test_config() -> Config {
        Config {
            immediate: ImmediateConfig {
                command_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_millis(200),
                retry_policy: RetryPolicy::default().never(),
                retry_interval: Duration::from_millis(500),
            },
            background: BackgroundConfig {
                timeout: Duration::from_millis(200),
                retry_policy: RetryPolicy::default()
                    .max_delay(Duration::from_millis(200))
                    .iter_add(Duration::from_secs(0))
                    .iter_mul(1.0)
                    .min_delay(Duration::from_millis(100))
                    .jitter(Duration::from_secs(0)),

                infinite_checks: true,
            },
        }
    }
    fn new_service(config: Config, tester: MockOnlineTester) -> NetworkMonitorService {
        _ = set_global_default(
            registry()
                .with(EnvFilter::new("debug"))
                .with(layer().with_test_writer()),
        );
        let mut service = NetworkMonitorService::new(config);
        service
            .start(&proton_task_service::Tokio::spawner(), Arc::new(tester))
            .unwrap();
        service
    }
}
