use crate::common::{DynSocket, IntoDyn};
use crate::rt::Dialer;
use crate::{ErrorKind, Result};
use async_io::Async;
use async_trait::async_trait;
use futures::TryFutureExt;
use std::net::{SocketAddr, TcpStream};

/// An async dialer.
#[derive(Debug, Default)]
pub struct AsyncDialer;

#[async_trait]
impl Dialer for AsyncDialer {
    async fn dial(&self, addr: SocketAddr) -> Result<DynSocket> {
        trace!(?addr, "dialing host");

        let sock = Async::<TcpStream>::connect(addr)
            .map_err(ErrorKind::dial)
            .await?;

        Ok(sock.into_dyn())
    }
}
