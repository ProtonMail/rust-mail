use crate::common::prelude::*;
use crate::http::common::pool::{Pool, PooledSender, TryGet};
use crate::http::prelude::*;
use crate::http::DohHostLayer;
use crate::util::TryRace;
use crate::{ErrorKind, Result};
use futures::TryFutureExt;
use std::sync::Arc;
use thiserror::Error;

/// Returned if no servers are provided to `HttpSender::sender_for`.
#[derive(Debug, Error)]
#[error("no servers available")]
pub struct NoServers;

/// An HTTP sender.
#[derive(Debug)]
pub struct HttpSender {
    servers: Vec<Server>,
    connector: DynHttpConnector,
    pool: Arc<Pool>,
}

impl HttpSender {
    /// Create a new HTTP sender.
    #[must_use]
    pub fn new(connector: impl IntoDyn<DynHttpConnector>, servers: impl Into<Vec<Server>>) -> Self {
        Self {
            servers: servers.into(),
            connector: connector.into_dyn().layer([DohHostLayer]),
            pool: Arc::default(),
        }
    }

    async fn send(&self, req: HttpReq) -> Result<HttpRes> {
        let (direct, indirect) = req
            .get_servers()
            .unwrap_or(&self.servers)
            .to_owned()
            .into_iter()
            .partition(Server::is_direct);

        self.sender_for(direct)
            .or_else(|_| self.sender_for(indirect))
            .and_then(|sender| self.send_with(sender, req))
            .await
    }

    async fn send_with(&self, sender: PooledSender, req: HttpReq) -> Result<HttpRes> {
        let timeout = req.get_allowed_time();

        match sender.send(req).with_timeout(timeout).map_err(ErrorKind::send).await {
            Ok(Ok(res)) => {
                sender.repool().await;
                Ok(res)
            }

            Ok(Err(err)) | Err(err) => {
                sender.unpool().await;
                Err(err)
            }
        }
    }

    async fn sender_for(&self, servers: Vec<Server>) -> Result<PooledSender> {
        if servers.is_empty() {
            return Err(ErrorKind::send(NoServers));
        }

        let mut pool = self.pool.lock().await;

        for server in &servers {
            pool = match pool.get(&server.endpoint) {
                TryGet::Some(sender) => return Ok(sender),
                TryGet::None(pool) => pool,
            };
        }

        let mut futs = Vec::new();

        for server in &servers {
            futs.push(
                self.connector
                    .connect(server)
                    .map_ok(move |sender| (&server.endpoint, sender)),
            );
        }

        futs.try_race().map_ok(|(e, s)| pool.insert(e, s)).await
    }
}

impl Sender<HttpReq, HttpRes> for HttpSender {
    #[instrument(level = "debug", skip(self), fields(%req))]
    fn send(&self, req: HttpReq) -> BoxFut<'_, Result<HttpRes>> {
        Box::pin(self.send(req))
    }
}
