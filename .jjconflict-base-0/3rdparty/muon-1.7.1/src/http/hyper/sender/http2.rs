use crate::common::prelude::*;
use crate::common::DynSocket;
use crate::http::hyper::compat::{HyperIo, HyperRt, HyperTimer};
use crate::http::hyper::sender::{fmt_req, is_retryable, ReadySend, ReadySendMut, SendWith};
use crate::http::{Body, Collect, HttpReq, HttpRes, Version};
use crate::rt::DynSpawner;
use crate::{ErrorKind, Result};
use async_trait::async_trait;
use futures::TryFutureExt;
use http::{Request, Response};
use hyper::client::conn::http2;
use hyper::client::conn::http2::SendRequest;
use hyper::rt::Executor;
use std::borrow::Borrow;

pub async fn new_http2_sender(
    sock: impl IntoDyn<DynSocket>,
    exec: impl IntoDyn<DynSpawner>,
    serv: impl Borrow<Server>,
    name: impl Borrow<Name>,
) -> Result<H2Sender> {
    let sock = HyperIo(sock.into_dyn());
    let exec = exec.into_dyn();
    let serv = serv.borrow().to_owned();
    let name = name.borrow().to_owned();

    H2Sender::connect(sock, HyperRt(exec), serv, name)
        .map_err(ErrorKind::connect)
        .await
}

/// An HTTP/2 sender implementation.
#[derive(Debug)]
pub struct H2Sender<B = Body> {
    sender: SendRequest<B>,
    server: Server,
    name: Name,
}

impl H2Sender {
    fn new(sender: SendRequest<Body>, server: Server, name: Name) -> Self {
        Self {
            sender,
            server,
            name,
        }
    }

    async fn connect(
        sock: HyperIo,
        exec: HyperRt,
        serv: Server,
        name: Name,
    ) -> hyper::Result<Self> {
        trace!(%serv, %name, "performing HTTP/2 handshake");

        let (sender, driver) = http2::Builder::new(exec.clone())
            .timer(HyperTimer)
            .keep_alive_interval(HTTP_KEEPALIVE)
            .handshake(sock)
            .await?;

        exec.execute(driver);

        Ok(Self::new(sender, serv, name))
    }

    async fn send(&self, req: HttpReq) -> Result<HttpRes> {
        trace!(%req, "sending with HTTP/2 sender");

        HttpReq::build(req, Version::HTTP_2, &self.server, &self.name)?
            .send_with(&self.sender)
            .map_ok(|res| HttpRes::new(self.server.clone(), self.name.clone(), res))
            .inspect_err(|e| error!(%e, "failed to send HTTP/2 request"))
            .map_err(|e| {
                if is_retryable(&e) {
                    ErrorKind::send(e).with_retryable(true)
                } else {
                    ErrorKind::send(e)
                }
            })
            .await
    }
}

impl Sender<HttpReq, HttpRes> for H2Sender {
    fn send(&self, req: HttpReq) -> BoxFut<'_, Result<HttpRes>> {
        Box::pin(self.send(req))
    }
}

impl<B> Drop for H2Sender<B> {
    fn drop(&mut self) {
        trace!("dropping H2Sender");
    }
}

#[async_trait]
impl<B: http_body::Body + Send + 'static> ReadySend<B> for SendRequest<B> {
    #[instrument(level = "debug", skip_all, fields(req = %fmt_req(&req)))]
    async fn ready_send(&self, req: Request<B>) -> hyper::Result<Response<Vec<u8>>> {
        trace!("sending request with HTTP/2 sender");
        let mut this = self.clone();

        trace!("waiting for HTTP/2 sender to be ready");
        this.ready().await?;

        trace!("HTTP/2 sender is ready, sending request");
        this.send_request(req).await?.collect().await
    }
}

#[async_trait]
impl<B: http_body::Body + Send + 'static> ReadySendMut<B> for SendRequest<B> {
    #[instrument(level = "debug", skip_all, fields(req = %fmt_req(&req)))]
    async fn ready_send_mut(&mut self, req: Request<B>) -> hyper::Result<Response<Vec<u8>>> {
        trace!("waiting for HTTP/2 sender to be ready");
        self.ready().await?;

        trace!("HTTP/2 sender is ready, sending request");
        self.send_request(req).await?.collect().await
    }
}
