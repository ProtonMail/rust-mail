use crate::common::{BoxFut, Sender, SenderLayer};
use crate::http::{HttpReq, HttpRes};
use crate::Result;
use futures_timer::Delay;
use muon_proc::autoimpl;
use std::borrow::Borrow;
use std::time::Duration;

/// A layer that retries if the remote server closes the connection.
#[must_use]
#[derive(Debug)]
pub struct OnSendRetryHandler;

impl OnSendRetryHandler {
    async fn on_send(&self, inner: &dyn Sender<HttpReq, HttpRes>, req: HttpReq) -> Result<HttpRes> {
        let mut err = match inner.send(req.clone()).await {
            Ok(res) => return Ok(res),
            Err(err) => err,
        };

        if !err.retryable() {
            return Err(err);
        }

        let Some(&policy) = req.get_retry_policy() else {
            return Err(err);
        };

        for delay in policy {
            warn!(?delay, "send operation failed, retrying");

            if err.retryable() {
                Delay::new(delay).await;
            } else {
                break;
            }

            err = match inner.send(req.clone()).await {
                Ok(res) => return Ok(res),
                Err(err) => err,
            };
        }

        Err(err)
    }
}

impl SenderLayer<HttpReq, HttpRes> for OnSendRetryHandler {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        Box::pin(self.on_send(inner, req))
    }
}

/// A layer that handles responses with status code 429.
#[must_use]
#[derive(Debug)]
pub struct Status429Handler;

impl Status429Handler {
    async fn on_send(&self, inner: &dyn Sender<HttpReq, HttpRes>, req: HttpReq) -> Result<HttpRes> {
        let mut res = inner.send(req.clone()).await?;

        if !res.is(429) {
            return Ok(res);
        }

        let Some(&policy) = req.get_retry_policy() else {
            return Ok(res);
        };

        for delay in policy {
            warn!(?delay, "rate limited, retrying");

            if res.is(429) {
                Delay::new(res.retry_after().max(delay)).await;
            } else {
                break;
            }

            res = inner.send(req.clone()).await?;
        }

        Ok(res)
    }
}

impl SenderLayer<HttpReq, HttpRes> for Status429Handler {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        Box::pin(self.on_send(inner, req))
    }
}

/// A layer that handles responses with status code 5xx.
#[must_use]
#[derive(Debug)]
pub struct Status5xxHandler;

impl Status5xxHandler {
    async fn on_send(&self, inner: &dyn Sender<HttpReq, HttpRes>, req: HttpReq) -> Result<HttpRes> {
        let mut res = inner.send(req.clone()).await?;

        if !res.status().is_server_error() {
            return Ok(res);
        }

        let Some(&policy) = req.get_retry_policy() else {
            return Ok(res);
        };

        for delay in policy {
            warn!(?delay, "server error, retrying");

            if res.status().is_server_error() {
                Delay::new(delay).await;
            } else {
                break;
            }

            res = inner.send(req.clone()).await?;
        }

        Ok(res)
    }
}

impl SenderLayer<HttpReq, HttpRes> for Status5xxHandler {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        Box::pin(self.on_send(inner, req))
    }
}

#[autoimpl]
trait RetryAfter: Borrow<HttpRes> {
    fn retry_after(&self) -> Duration {
        self.borrow()
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or_default()
    }
}
