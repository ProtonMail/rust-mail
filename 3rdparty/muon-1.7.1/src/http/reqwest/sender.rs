use crate::common::prelude::*;
use crate::http::{DynHttpSender, HttpReq, HttpRes, Version};
use crate::{Error, ErrorKind, Result};
use bytes::Bytes;
use futures::TryFutureExt;
use http::response::Builder;
use http::HeaderMap;
use muon_proc::autoimpl;
use reqwest::Client;
use std::borrow::Borrow;
use thiserror::Error;

pub fn new_sender(server: impl Borrow<Server>, name: impl Borrow<Name>) -> DynHttpSender {
    let client = Client::new();
    let server = server.borrow().to_owned();
    let name = name.borrow().to_owned();

    ReqwestSender {
        client,
        server,
        name,
    }
    .into_dyn()
}

#[derive(Debug)]
struct ReqwestSender {
    client: Client,
    server: Server,
    name: Name,
}

impl ReqwestSender {
    async fn send(&self, req: HttpReq) -> Result<HttpRes, ReqwestSendErr> {
        // Build the request.
        let (head, body) = req
            .build(Version::HTTP_11, &self.server, &self.name)?
            .into_parts();

        // Send the request.
        let res = (self.client)
            .request(head.method, head.uri.to_string())
            .headers(head.headers)
            .body(Bytes::from(body))
            .send()
            .await?;

        // Build the response.
        let res = http::Response::builder()
            .status(res.status())
            .headers(res.headers())
            .body(res.bytes().await?.to_vec())?
            .into();

        Ok(HttpRes::new(self.server.clone(), self.name.clone(), res))
    }
}

impl Sender<HttpReq, HttpRes> for ReqwestSender {
    fn send(&self, req: HttpReq) -> BoxFut<Result<HttpRes>> {
        Box::pin(self.send(req).err_into())
    }
}

#[autoimpl]
trait BuilderExt: Into<Builder> + Sized {
    fn headers(self, headers: impl Borrow<HeaderMap>) -> Builder {
        let mut this = self.into();

        for (k, v) in headers.borrow().iter() {
            this = this.header(k, v);
        }

        this
    }
}

mod errors {
    use super::*;

    /// An error that can occur when using the WASM HTTP client.
    #[derive(Debug, Error)]
    #[error(transparent)]
    pub enum ReqwestSendErr {
        Reqwest(#[from] reqwest::Error),
        Http(#[from] http::Error),
        Inner(#[from] Error),
    }

    impl From<ReqwestSendErr> for Error {
        fn from(err: ReqwestSendErr) -> Self {
            if let ReqwestSendErr::Inner(err) = err {
                err.map_kind(ErrorKind::Send)
            } else {
                ErrorKind::send(err)
            }
        }
    }
}

use self::errors::*;
