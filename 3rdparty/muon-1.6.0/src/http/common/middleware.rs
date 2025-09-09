//! HTTP middleware.

use crate::common::prelude::*;
use crate::headers::DohHostHeader;
use crate::http::prelude::*;
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Creates a layer that sets a header on every outgoing request.
#[must_use]
pub fn set_header(header: impl AsHeader + Send + Sync + 'static) -> DynHttpSenderLayer {
    SetHeaderLayer(Arc::new(header)).into_dyn()
}

/// Creates a layer that sets the retry policy on every outgoing request
/// (if not already set).
#[must_use]
pub fn set_retry_policy(policy: RetryPolicy) -> DynHttpSenderLayer {
    SetRetryPolicyLayer(policy).into_dyn()
}

/// A layer that sets a header on every outgoing request.
struct SetHeaderLayer<T>(Arc<T>);

impl<T: AsHeader> SetHeaderLayer<T> {
    async fn on_send(&self, inner: &dyn Sender<HttpReq, HttpRes>, req: HttpReq) -> Result<HttpRes> {
        inner.send(req.header(self.0.as_ref())).await
    }
}

impl<T> SenderLayer<HttpReq, HttpRes> for SetHeaderLayer<T>
where
    T: AsHeader + Send + Sync + 'static,
{
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        Box::pin(self.on_send(inner, req))
    }
}

/// A layer that sets the retry policy on every outgoing request.
struct SetRetryPolicyLayer(RetryPolicy);

impl SetRetryPolicyLayer {
    async fn on_send(&self, inner: &dyn Sender<HttpReq, HttpRes>, req: HttpReq) -> Result<HttpRes> {
        if req.get_retry_policy().is_none() {
            Ok(inner.send(req.retry_policy(self.0)).await?)
        } else {
            Ok(inner.send(req).await?)
        }
    }
}

impl SenderLayer<HttpReq, HttpRes> for SetRetryPolicyLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        Box::pin(self.on_send(inner, req))
    }
}

/// Adds the DoH host header to requests made to indirect hosts.
///
/// The `x-pm-doh-host` header should hold the direct name of the host.
/// If we connect to an indirect host, we convert the indirect name back to a
/// direct name before applying the layer. For example, when connecting to the
/// server `dNVQWS3BNMFYGSLTQOJXXI33OFZWWK.protonpro.xyz`, the `x-pm-doh-host`
/// header should be set to `verify.proton.me`.
#[derive(Debug)]
pub struct DohHostLayer;

#[async_trait]
impl ConnectorLayer<HttpReq, HttpRes> for DohHostLayer {
    async fn on_connect(
        &self,
        inner: &dyn Connector<HttpReq, HttpRes>,
        server: &Server,
    ) -> Result<DynSender<HttpReq, HttpRes>> {
        let sender = inner.connect(server).await?;

        if server.host().is_direct() {
            return Ok(sender);
        }

        let Some(host) = server.host().to_direct() else {
            return Ok(sender);
        };

        Ok(sender.layer([set_header(DohHostHeader::new(host))]))
    }
}
