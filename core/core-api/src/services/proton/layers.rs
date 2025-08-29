use std::sync::Arc;
use std::time::Duration;

use chrono::DateTime;
use cookie::{Cookie, CookieJar};
use muon::common::{BoxFut, Sender, SenderLayer, ServiceType};
use muon::{ProtonRequest, ProtonResponse, Result as MuonResult};
use proton_crypto_account::proton_crypto::crypto::UnixTimestamp;
use tokio::sync::RwLock;

use crate::crypto_clock::server_crypto_clock;

pub struct SetCryptoClockLayer;

impl SetCryptoClockLayer {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let response = inner.send(req).await?;

        if let Some(date) = response
            .headers()
            .get("date")
            .and_then(|response_time_header| response_time_header.to_str().ok())
            .and_then(|response_time| DateTime::parse_from_rfc2822(response_time).ok())
            .and_then(|parsed_server_time| parsed_server_time.timestamp().try_into().ok())
            .map(UnixTimestamp)
        {
            server_crypto_clock().update_clock(date);
        }

        Ok(response)
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetCryptoClockLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

pub struct SetDefaultServiceTypeLayer;

impl SetDefaultServiceTypeLayer {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        let req = if req.get_service_type().is_none() {
            req.service_type(ServiceType::default(), true)
        } else {
            req
        };

        inner.send(req).await
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetDefaultServiceTypeLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

pub struct SetDefaultTimeoutLayer;

impl SetDefaultTimeoutLayer {
    async fn on_send<S>(&self, inner: &S, req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

        // NOTE: This is not a bug! Muon logs a warning if no timeout is explicitly set;
        // this workaround sets the timeout explicitly if it was not already set to a
        // non-default value earlier in the layer stack.
        let req = if req.get_allowed_time() == DEFAULT_TIMEOUT {
            req.allowed_time(DEFAULT_TIMEOUT)
        } else {
            req
        };

        inner.send(req).await
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for SetDefaultTimeoutLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}

pub struct CookieJarLayer {
    jar: Arc<RwLock<CookieJar>>,
}

impl CookieJarLayer {
    /// Create a new cookie jar layer.
    pub fn new(jar: CookieJar) -> Self {
        Self {
            jar: Arc::new(RwLock::new(jar)),
        }
    }
}

#[allow(clippy::similar_names)]
impl CookieJarLayer {
    async fn on_send<S>(&self, inner: &S, mut req: ProtonRequest) -> MuonResult<ProtonResponse>
    where
        S: Sender<ProtonRequest, ProtonResponse> + ?Sized,
    {
        for cookie in self.jar.read().await.iter() {
            req = req.header(("cookie", cookie.value()));
        }

        let res = inner.send(req).await?;

        for cookie in res.headers().get_all("set-cookie") {
            if let Ok(cookie) = cookie.to_str()
                && let Ok(cookie) = Cookie::parse(cookie)
            {
                self.jar.write().await.add(cookie.into_owned());
            }
        }

        Ok(res)
    }
}

impl SenderLayer<ProtonRequest, ProtonResponse> for CookieJarLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<ProtonRequest, ProtonResponse>,
        req: ProtonRequest,
    ) -> BoxFut<'a, MuonResult<ProtonResponse>> {
        Box::pin(self.on_send(inner, req))
    }
}
