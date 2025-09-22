use crate::common::{DynSocket, IntoDyn, Socket, WithTimeout, HTTP_PROXY_CONNECT};
use crate::http::hyper::compat::{HyperIo, HyperRt};
use crate::http::hyper::sender::SendWith;
use crate::http::Body;
use crate::rt::DynSpawner;
use crate::{Error, ErrorKind, Result};
use async_trait::async_trait;
use hyper::client::conn::http1;
use hyper::rt::Executor;
use hyper::upgrade;
use muon_proc::autoimpl;
use std::net::{IpAddr, SocketAddr};
use thiserror::Error;

/// A type that can be converted into a tunnel.
#[async_trait]
pub trait HttpTunnel {
    /// Create a tunnel to the given server.
    async fn tunnel(self, addr: SocketAddr, exec: &DynSpawner) -> Result<DynSocket>;
}

/// A type that can be converted into a tunnel.
#[autoimpl]
pub trait HttpTunnelExt: HttpTunnel + Sized {
    /// Create a tunnel to the given address and port.
    async fn tunnel_addr(self, addr: IpAddr, port: u16, exec: &DynSpawner) -> Result<DynSocket> {
        self.tunnel((addr, port).into(), exec).await
    }
}

#[async_trait]
impl<This: Socket> HttpTunnel for This {
    async fn tunnel(self, addr: SocketAddr, exec: &DynSpawner) -> Result<DynSocket> {
        let sock = HyperIo(self.into_dyn());
        let exec = HyperRt(exec.into_dyn());
        let addr = addr.to_string();

        Ok(tunnel(sock, exec, addr).await?)
    }
}

async fn tunnel(sock: HyperIo, exec: HyperRt, addr: String) -> Result<DynSocket, TunnelErr> {
    let (mut sender, driver) = http1::handshake(sock).await?;

    exec.execute(driver.with_upgrades());

    let res = http::Request::connect(addr)
        .body(Body::default())?
        .send_with_mut(&mut sender)
        .with_timeout(HTTP_PROXY_CONNECT)
        .await??;

    Ok(HyperIo(upgrade::on(res).await?).into_dyn())
}

mod errors {
    use super::*;
    use crate::common::Timeout;

    #[derive(Debug, Error)]
    #[error("tunnel: {0}")]
    pub enum TunnelErr {
        Hyper(#[from] hyper::Error),
        Http(#[from] http::Error),
        Timeout(#[from] Timeout),
    }

    impl From<TunnelErr> for Error {
        fn from(err: TunnelErr) -> Self {
            ErrorKind::connect(err)
        }
    }
}

use self::errors::*;
