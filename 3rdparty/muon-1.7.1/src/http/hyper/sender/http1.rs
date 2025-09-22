use crate::common::prelude::*;
use crate::common::DynSocket;
use crate::http::hyper::compat::{HyperIo, HyperRt};
use crate::http::hyper::sender::{fmt_req, is_retryable, ReadySend, ReadySendMut, SendWith};
use crate::http::{Body, Collect, HttpReq, HttpRes, Version};
use crate::rt::DynSpawner;
use crate::{ErrorKind, Result};
use async_trait::async_trait;
use futures::lock::Mutex;
use futures::TryFutureExt;
use http::{Request, Response};
use hyper::client::conn::http1;
use hyper::client::conn::http1::SendRequest;
use hyper::rt::Executor;
use std::borrow::Borrow;

pub async fn new_http1_sender(
    sock: impl IntoDyn<DynSocket>,
    exec: impl IntoDyn<DynSpawner>,
    serv: impl Borrow<Server>,
    name: impl Borrow<Name>,
) -> Result<H1Sender> {
    let sock = HyperIo(sock.into_dyn());
    let exec = HyperRt(exec.into_dyn());
    let serv = serv.borrow().to_owned();
    let name = name.borrow().to_owned();

    H1Sender::connect(sock, exec, serv, name)
        .map_err(ErrorKind::connect)
        .await
}

#[derive(Debug)]
pub struct H1Sender {
    sender: Mutex<SendRequest<Body>>,
    server: Server,
    name: Name,
}

impl H1Sender {
    fn new(sender: SendRequest<Body>, server: Server, name: Name) -> Self {
        let sender = Mutex::new(sender);

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
        trace!(%serv, %name, "performing HTTP/1.1 handshake");

        let (sender, driver) = http1::handshake(sock).await?;

        exec.execute(driver.with_upgrades());

        Ok(Self::new(sender, serv, name))
    }

    async fn send(&self, req: HttpReq) -> Result<HttpRes> {
        trace!(%req, "sending with HTTP/1.1 sender");

        HttpReq::build(req, Version::HTTP_11, &self.server, &self.name)?
            .send_with(&self.sender)
            .map_ok(|res| HttpRes::new(self.server.clone(), self.name.clone(), res))
            .inspect_err(|e| error!(%e, "failed to send HTTP/1.1 request"))
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

impl Sender<HttpReq, HttpRes> for H1Sender {
    fn send(&self, req: HttpReq) -> BoxFut<'_, Result<HttpRes>> {
        Box::pin(self.send(req))
    }
}

impl Drop for H1Sender {
    fn drop(&mut self) {
        trace!("dropping H1Sender");
    }
}

#[async_trait]
impl<B: http_body::Body + Send + 'static> ReadySend<B> for Mutex<SendRequest<B>> {
    #[instrument(level = "debug", skip_all, fields(req = %fmt_req(&req)))]
    async fn ready_send(&self, req: Request<B>) -> hyper::Result<Response<Vec<u8>>> {
        trace!("sending request with HTTP/1.1 sender");
        let mut this = self.lock().await;

        trace!("waiting for HTTP/1.1 sender to be ready");
        this.ready().await?;

        trace!("HTTP/1.1 sender is ready, sending request");
        this.send_request(req).await?.collect().await
    }
}

#[async_trait]
impl<B: http_body::Body + Send + 'static> ReadySendMut<B> for SendRequest<B> {
    #[instrument(level = "debug", skip_all, fields(req = %fmt_req(&req)))]
    async fn ready_send_mut(&mut self, req: Request<B>) -> hyper::Result<Response<Vec<u8>>> {
        trace!("waiting for HTTP/1.1 sender to be ready");
        self.ready().await?;

        trace!("HTTP/1.1 sender is ready, sending request");
        self.send_request(req).await?.collect().await
    }
}
