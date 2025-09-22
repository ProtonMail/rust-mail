use crate::common::prelude::*;
use crate::http::reqwest::sender::new_sender;
use crate::http::{DynHttpSender, HttpReq, HttpRes};
use crate::{Error, ErrorKind, Result};
use async_trait::async_trait;
use thiserror::Error;
use url::ParseError as ParseUrlErr;

/// An HTTP connector using reqwest.
#[derive(Debug)]
pub struct ReqwestConnector;

impl ReqwestConnector {
    async fn connect(&self, server: &Server) -> Result<DynHttpSender, ReqwestConnectErr> {
        Ok(new_sender(server, server.name()))
    }
}

#[async_trait]
impl Connector<HttpReq, HttpRes> for ReqwestConnector {
    async fn connect(&self, server: &Server) -> Result<DynHttpSender> {
        Ok(self.connect(server).await?)
    }
}

mod errors {
    use super::*;

    #[derive(Debug, Error)]
    #[error("connect: {0}")]
    pub enum ReqwestConnectErr {
        Url(#[from] ParseUrlErr),
    }

    impl From<ReqwestConnectErr> for Error {
        fn from(err: ReqwestConnectErr) -> Self {
            ErrorKind::connect(err)
        }
    }
}

use self::errors::*;
