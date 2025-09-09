use crate::common::{BoxFut, Sender, SenderLayer};
use crate::http::{HttpReq, HttpRes};
use crate::Result;
use tracing::Level;

/// Logs an event at the given level.
macro_rules! dyn_event {
    ($lvl:expr, $($arg:tt)+) => {
        match $lvl {
            Level::TRACE => tracing::trace!($($arg)+),
            Level::DEBUG => tracing::debug!($($arg)+),
            Level::INFO => tracing::info!($($arg)+),
            Level::WARN => tracing::warn!($($arg)+),
            Level::ERROR => tracing::error!($($arg)+),
        }
    };
}

/// A layer that logs requests and responses using the `tracing` crate.
#[must_use]
#[derive(Debug)]
pub struct DebugLogger(Level);

impl DebugLogger {
    /// Create a new tracing logger layer.
    pub fn new(level: impl Into<Level>) -> Self {
        DebugLogger(level.into())
    }

    /// Create a new `TRACE` tracing logger layer.
    pub fn trace() -> Self {
        Self::new(Level::TRACE)
    }

    /// Create a new `DEBUG` tracing logger layer.
    pub fn debug() -> Self {
        Self::new(Level::DEBUG)
    }

    /// Create a new `INFO` tracing logger layer.
    pub fn info() -> Self {
        Self::new(Level::INFO)
    }

    async fn on_send(&self, inner: &dyn Sender<HttpReq, HttpRes>, req: HttpReq) -> Result<HttpRes> {
        dyn_event!(self.0, ?req, "sending request");

        let res = inner.send(req).await?;

        dyn_event!(self.0, ?res, "received response");

        Ok(res)
    }
}

impl SenderLayer<HttpReq, HttpRes> for DebugLogger {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        Box::pin(self.on_send(inner, req))
    }
}

/// A layer that logs requests and responses using the `tracing` crate.
#[must_use]
#[derive(Debug)]
pub struct DisplayLogger(Level);

impl DisplayLogger {
    /// Create a new tracing logger layer.
    pub fn new(level: impl Into<Level>) -> Self {
        DisplayLogger(level.into())
    }

    /// Create a new `TRACE` tracing logger layer.
    pub fn trace() -> Self {
        Self::new(Level::TRACE)
    }

    /// Create a new `DEBUG` tracing logger layer.
    pub fn debug() -> Self {
        Self::new(Level::DEBUG)
    }

    /// Create a new `INFO` tracing logger layer.
    pub fn info() -> Self {
        Self::new(Level::INFO)
    }

    async fn on_send(&self, inner: &dyn Sender<HttpReq, HttpRes>, req: HttpReq) -> Result<HttpRes> {
        dyn_event!(self.0, %req, "sending request");

        let res = inner.send(req).await?;

        dyn_event!(self.0, %res, "received response");

        Ok(res)
    }
}

impl SenderLayer<HttpReq, HttpRes> for DisplayLogger {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        Box::pin(self.on_send(inner, req))
    }
}
