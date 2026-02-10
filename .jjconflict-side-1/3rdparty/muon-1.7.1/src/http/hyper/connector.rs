//! An HTTP connector, responsible for returning a sender for a given server.

use crate::common::prelude::*;
use crate::common::Scheme::*;
use crate::http::hyper::sender::{new_http1_sender, new_http2_sender};
use crate::http::hyper::tunnel::HttpTunnelExt;
use crate::http::{DynHttpSender, HttpReq, HttpRes};
use crate::rt::{DynDialer, DynResolver, DynSpawner};
use crate::tls::{Alpn, DynTlsUpgrader, TlsSocketExt};
use crate::util::TryRace;
use crate::{Error, ErrorKind, Result};
use async_trait::async_trait;
use futures::TryFutureExt;
use std::fmt::{Display, Formatter, Result as FmtResult};
use thiserror::Error;
use url::ParseError as ParseUrlErr;

// The ALPN protocols supported by the client.
const H2: Alpn = Alpn::new(b"h2");
const H1: Alpn = Alpn::new(b"http/1.1");
const H1H2: &[Alpn] = &[H1, H2];

/// An HTTP connector, responsible for returning a sender for a given server.
#[derive(Debug)]
#[allow(unused)]
pub struct HyperConnector {
    spawner: DynSpawner,
    resolver: DynResolver,
    dialer: DynDialer,
    upgrader: DynTlsUpgrader,
    proxy: DynProxy,
}

impl HyperConnector {
    /// Create a new HTTP connector.
    pub fn new(
        spawner: impl IntoDyn<DynSpawner>,
        resolver: impl IntoDyn<DynResolver>,
        dialer: impl IntoDyn<DynDialer>,
        upgrader: impl IntoDyn<DynTlsUpgrader>,
        proxy: impl IntoDyn<DynProxy>,
    ) -> Self {
        Self {
            spawner: spawner.into_dyn(),
            resolver: resolver.into_dyn(),
            dialer: dialer.into_dyn(),
            upgrader: upgrader.into_dyn(),
            proxy: proxy.into_dyn(),
        }
    }

    async fn connect(&self, srv: &Server) -> Result<DynHttpSender, HyperConnectErr> {
        trace!("resolving server");

        // the resolve timeout is set deep in the DNS implementation because it depends
        // on the type of resolution we use. DNS over UDP does not have the same timeout
        // as DoT
        let srv_addr = match self.resolve(srv.host()).await {
            Ok(addr) => {
                trace!("resolved {} server address(es)", addr.len());
                addr
            }

            Err(err) => {
                error!(%err, "failed to resolve server");
                return Err(err);
            }
        };

        trace!("checking for proxy");
        let prx = if let Some(prx) = self.proxy.proxy(&srv.endpoint) {
            trace!("resolving proxy");
            match self.resolve(&prx.host).await {
                Ok(addr) => {
                    trace!("resolved {} proxy address(es)", addr.len());
                    Some((prx, addr))
                }

                Err(err) => {
                    error!(%err, "failed to resolve proxy");
                    return Err(err);
                }
            }
        } else {
            None
        };

        trace!("dialing server");
        let (sock, name, alpn) = match self.dial(&srv.endpoint, &srv_addr, &prx).await {
            Ok((sock, name, alpn)) => {
                trace!(%name, "socket connected to server");
                (sock, name, alpn)
            }

            Err(err) => {
                error!(%err, "failed to dial server");
                return Err(err);
            }
        };

        Ok(match alpn {
            None => {
                trace!("defaulting to HTTP/1.1");
                (new_http1_sender(sock, &self.spawner, srv, name).await?).into_dyn()
            }

            Some(H2) => {
                trace!("connecting to server using HTTP/2");
                (new_http2_sender(sock, &self.spawner, srv, name).await?).into_dyn()
            }

            Some(H1) => {
                trace!("connecting to server using HTTP/1.1");
                (new_http1_sender(sock, &self.spawner, srv, name).await?).into_dyn()
            }

            Some(alpn) => {
                error!(%alpn, "unsupported ALPN returned by server");
                Err(AlpnErr(alpn))?
            }
        })
    }

    async fn resolve(&self, host: &Host) -> Result<Vec<Addr>, HyperConnectErr> {
        match self.resolver.resolve(host).await?.into_res() {
            Ok(addr) => Ok(addr
                .into_iter()
                .inspect(|addr| trace!("found address for {host}: {addr}"))
                .collect()),

            Err(err) => {
                error!(%err, "failed to resolve host");
                Err(err)?
            }
        }
    }

    async fn dial<'a>(
        &self,
        srv: &Endpoint,
        srv_addr: &'a [Addr],
        prx: &Option<(Endpoint, Vec<Addr>)>,
    ) -> Result<(DynSocket, &'a Name, Option<Alpn>), HyperConnectErr> {
        srv_addr
            .iter()
            .map(|addr| self.dial_addr(srv, addr, prx))
            .try_race()
            .await
    }

    async fn dial_addr<'a>(
        &self,
        srv: &Endpoint,
        srv_addr: &'a Addr,
        prx: &Option<(Endpoint, Vec<Addr>)>,
    ) -> Result<(DynSocket, &'a Name, Option<Alpn>), HyperConnectErr> {
        if let Some((prx, prx_addr)) = &prx {
            self.dial_addr_proxy(srv, srv_addr, prx, prx_addr)
                .map_ok(|(sock, alpn)| (sock, &srv_addr.name, alpn))
                .await
        } else {
            self.dial_addr_direct(srv, srv_addr, H1H2)
                .map_ok(|(sock, alpn)| (sock, &srv_addr.name, alpn))
                .await
        }
    }

    async fn dial_addr_direct(
        &self,
        srv: &Endpoint,
        addr: &Addr,
        alpn: &[Alpn],
    ) -> Result<(DynSocket, Option<Alpn>), HyperConnectErr> {
        let name = &addr.name;
        let host = &srv.host;

        trace!("dialing {addr} directly");
        let sock = self
            .dialer
            .dial_addr(addr.ip, srv.port)
            .with_timeout(TCP_CONNECT)
            .await??;

        let (sock, alpn) = if let Https = srv.scheme {
            trace!("upgrading to TLS");
            sock.upgrade(&self.upgrader, host, name, alpn)
                .with_timeout(TLS_HANDSHAKE)
                .await??
        } else {
            trace!("not upgrading to TLS");
            (sock, None)
        };

        Ok((sock, alpn))
    }

    async fn dial_addr_proxy(
        &self,
        srv: &Endpoint,
        srv_addr: &Addr,
        prx: &Endpoint,
        prx_addr: &[Addr],
    ) -> Result<(DynSocket, Option<Alpn>), HyperConnectErr> {
        let name = &srv_addr.name;
        let host = &srv.host;

        let mut errors: Vec<Error> = Vec::new();

        for prx_addr in prx_addr {
            let sock = match self.dial_addr_direct(prx, prx_addr, &[H1]).await {
                Ok((sock, _)) => {
                    trace!("socket connected to {prx_addr}");
                    sock
                }

                Err(err) => {
                    errors.push(err.into());
                    continue;
                }
            };

            let sock = match sock.tunnel_addr(srv_addr.ip, srv.port, &self.spawner).await {
                Ok(sock) => {
                    trace!("socket tunneled to {srv_addr}");
                    sock
                }

                Err(err) => {
                    errors.push(err);
                    continue;
                }
            };

            let (sock, alpn) = if let Https = srv.scheme {
                trace!("upgrading to TLS");
                sock.upgrade(&self.upgrader, host, name, H1H2).await?
            } else {
                trace!("not upgrading to TLS");
                (sock, None)
            };

            return Ok((sock, alpn));
        }

        Err(DialErr(errors))?
    }
}

#[async_trait]
impl Connector<HttpReq, HttpRes> for HyperConnector {
    #[instrument(level = "debug", skip(self), fields(%server))]
    async fn connect(&self, server: &Server) -> Result<DynHttpSender> {
        trace!("connecting to server");

        Ok(self.connect(server).await?)
    }
}

mod errors {
    use super::*;

    #[derive(Debug, Error)]
    #[error("unsupported ALPN returned by server: {0:?}")]
    pub struct AlpnErr(pub Alpn);

    #[derive(Debug, Error)]
    pub struct DialErr(pub Vec<Error>);

    impl Display for DialErr {
        fn fmt(&self, f: &mut Formatter) -> FmtResult {
            if let Some(err) = self.0.last() {
                write!(f, "{err}")
            } else {
                write!(f, "no addresses resolved")
            }
        }
    }

    #[derive(Debug, Error)]
    #[error("connect: {0}")]
    pub enum HyperConnectErr {
        Url(#[from] ParseUrlErr),
        Alpn(#[from] AlpnErr),
        Dial(#[from] DialErr),
        Timeout(#[from] Timeout),
        Inner(#[from] Error),
    }

    impl From<HyperConnectErr> for Error {
        fn from(err: HyperConnectErr) -> Self {
            if let HyperConnectErr::Inner(err) = err {
                err.map_kind(ErrorKind::Connect)
            } else {
                ErrorKind::connect(err)
            }
        }
    }
}

use self::errors::*;
