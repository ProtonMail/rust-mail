use crate::services::Service;
use crate::{Context, CoreContextError};
use anyhow::anyhow;
use async_trait::async_trait;
use parking_lot::RwLock;
use proton_action_queue::queue::{OnlineStatusWaiter, OnlineStatusWaiterBuilder};
use proton_core_api::connection_status::ConnectionStatus;
use proton_core_api::exports::RetryPolicy;
use proton_core_api::services::proton::ProtonCore;
use proton_core_api::session::Session;
use proton_network_monitor_service::{
    Config, ConnectionMonitor, NetworkMonitorService as ProtonNetworkMonitorService,
    NetworkStatusObserver, OnlineTester, OsNetworkStatus, OsNetworkStatusObserver,
    RequestNetworkStatus,
};
use std::sync::{Arc, Weak};
use std::time::Duration;

pub struct NetworkMonitorService {
    //TODO: remove once context can initialize with &mut self
    service: RwLock<ProtonNetworkMonitorService>,
    ctx: Weak<Context>,
}

impl NetworkMonitorService {
    #[must_use]
    pub fn new(ctx: Weak<Context>, config: Config) -> Self {
        Self {
            service: RwLock::new(ProtonNetworkMonitorService::new(config)),
            ctx,
        }
    }

    pub fn new_connection_monitor(&self) -> ConnectionMonitor {
        self.service.read().new_connection_monitor()
    }

    pub fn network_status_observer(&self) -> NetworkStatusObserver {
        self.service.read().network_status_observer()
    }

    pub fn os_network_status_observer(&self) -> OsNetworkStatusObserver {
        self.service.read().os_network_status_observer()
    }

    pub fn is_os_online(&self) -> bool {
        self.service.read().is_os_online()
    }
    pub fn is_os_offline(&self) -> bool {
        !self.service.read().is_os_online()
    }

    pub async fn check_now(&self) -> RequestNetworkStatus {
        let request = self.service.read().check_now_deferred();
        request.await
    }

    /// Networks status that uses both the os network and the request status reports.
    pub fn combined_status(&self) -> ConnectionStatus {
        self.service
            .read()
            .network_status_observer()
            .status()
            .into()
    }

    /// Network status that only uses the os network status report.
    pub fn os_status(&self) -> ConnectionStatus {
        self.service
            .read()
            .os_network_status_observer()
            .status()
            .into()
    }

    pub fn update_os_network_status(&self, status: OsNetworkStatus) {
        self.service.read().update_os_network_status(status);
    }
}

#[async_trait]
impl Service for NetworkMonitorService {
    type Error = CoreContextError;

    async fn init(&self) -> Result<(), Self::Error> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(CoreContextError::Other(anyhow!(
                "Could not upgrade context"
            )));
        };

        let connection_monitor = self.service.read().new_connection_monitor();
        let client = ctx
            .new_network_monitor_api_session(connection_monitor)
            .await?;

        let mut service = self.service.write();
        service.start(&ctx.spawner(), Arc::new(CoreOnlineTester(client)))?;
        Ok(())
    }
}

struct CoreOnlineTester(Session);

#[async_trait]
impl OnlineTester for CoreOnlineTester {
    async fn check(&self, timeout: Duration) -> RequestNetworkStatus {
        match self
            .0
            .get_tests_ping(Some(timeout), Some(RetryPolicy::default().never()))
            .await
            .inspect_err(|e| tracing::error!("Online check failed: {e:?}"))
        {
            Err(e) if e.is_server_unreachable() => RequestNetworkStatus::ServerUnreachable,
            Err(e) if e.is_network_failure() => RequestNetworkStatus::Offline,
            _ => RequestNetworkStatus::Online,
        }
    }
}

impl OnlineStatusWaiterBuilder for Context {
    fn build(&self) -> Box<dyn OnlineStatusWaiter> {
        Box::new(NetworkMonitorServiceOnlineStatusWaiter(
            self.network_monitor_service().network_status_observer(),
        ))
    }
}

struct NetworkMonitorServiceOnlineStatusWaiter(NetworkStatusObserver);

#[async_trait]
impl OnlineStatusWaiter for NetworkMonitorServiceOnlineStatusWaiter {
    async fn wait_until_online(&mut self) {
        self.0.wait_until_online().await;
    }
}
