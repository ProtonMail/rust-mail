use crate::common::prelude::*;
use crate::common::IntoDyn;
use crate::dns::{fmt_msg, impl_dns_service, Dns, DnsResolver, DynDnsService};
use crate::rt::{DynResolver, DynSpawner, SpawnerExt};
use crate::{Error, ErrorKind, Result};
use async_io::Async;
use async_trait::async_trait;
use derive_more::Display;
use futures::prelude::*;
use futures_timer::Delay;
use hickory_client::client::Client;
use hickory_client::proto::{udp, DnsHandle};
use hickory_proto::op::Message;
use hickory_proto::runtime::{RuntimeProvider, Spawn, Time};
use hickory_proto::udp::UdpClientStream;
use hickory_proto::xfer::DnsResponse;
use hickory_proto::{tcp, ProtoError};
use pin_project::pin_project;
use std::io::ErrorKind::TimedOut;
use std::io::Result as IoResult;
use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::Duration;

/// A poll result for an I/O operation.
type PollIoRes<T> = Poll<IoResult<T>>;

/// A DNS client that uses a public DNS server.
#[derive(Debug)]
pub struct DnsClient(DnsConnector);

impl DnsClient {
    /// Create a new DNS client that uses the given server.
    ///
    /// # Errors
    ///
    /// Returns an error if the client cannot be created.
    pub fn new<D, S>(service: D, spawner: S) -> Self
    where
        D: IntoDyn<DynDnsService>,
        S: IntoDyn<DynSpawner>,
    {
        DnsClient(DnsConnector::new(
            service.into_dyn().addr(),
            spawner.into_dyn(),
        ))
    }

    async fn query(&self, msg: Message) -> Result<Vec<Message>, DnsClientError> {
        trace!("connecting to DNS server");
        let client = self.0.connect().await?;

        trace!("sending DNS query");
        let res = client.send(msg);
        let msg = res.map_ok(DnsResponse::into_message);

        Ok(msg.try_collect().with_timeout(DNS_UDP_QUERY).await??)
    }
}

#[async_trait]
impl Dns for DnsClient {
    #[instrument(level = "debug", skip(self), fields(msg = %fmt_msg(&msg)))]
    async fn query(&self, msg: Message) -> Result<Vec<Message>> {
        trace!("performing DNS query");

        match self.query(msg).await {
            Ok(res) => {
                trace!("received DNS response");
                Ok(res)
            }

            Err(err) => {
                error!(%err, "failed to perform DNS query");
                Err(err)?
            }
        }
    }
}

impl IntoDyn<DynResolver> for DnsClient {
    fn into_dyn(self) -> DynResolver {
        DnsResolver::new(self).into_dyn()
    }
}

/// The Google public DNS service.
#[derive(Debug, Display)]
pub struct GoogleDns;

/// The Cloudflare public DNS service.
#[derive(Debug, Display)]
pub struct CloudflareDns;

/// The Quad9 public DNS service.
#[derive(Debug, Display)]
pub struct Quad9Dns;

// Implement the DNS service trait.
impl_dns_service! {
    CloudflareDns => ([1, 1, 1, 1], 53),
    GoogleDns => ([8, 8, 8, 8], 53),
    Quad9Dns => ([9, 9, 9, 9], 53),
}

#[derive(Debug)]
struct DnsConnector {
    address: SocketAddr,
    spawner: DynSpawner,
}

impl DnsConnector {
    fn new(address: SocketAddr, spawner: DynSpawner) -> Self {
        Self { address, spawner }
    }

    async fn connect(&self) -> Result<Client, ProtoError> {
        let rt = HickoryRuntimeProvider(self.spawner.clone());
        let stream = UdpClientStream::builder(self.address, rt).build();
        let (client, driver) = Client::connect(stream).await?;
        self.spawner.spawn_any(driver.map(|_| ()));

        Ok(client)
    }
}

#[derive(Debug)]
struct UdpSocket(Async<net::UdpSocket>);

#[async_trait]
impl udp::UdpSocket for UdpSocket {
    async fn connect(addr: SocketAddr) -> IoResult<Self> {
        let bind = match addr {
            SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
            SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
        };

        Self::connect_with_bind(addr, bind).await
    }

    async fn connect_with_bind(_: SocketAddr, bind: SocketAddr) -> IoResult<Self> {
        Self::bind(bind).await
    }

    async fn bind(addr: SocketAddr) -> IoResult<Self> {
        Ok(Self(Async::<net::UdpSocket>::bind(addr)?))
    }
}

impl udp::DnsUdpSocket for UdpSocket {
    type Time = DnsTime;

    fn poll_recv_from(&self, cx: &mut Context, buf: &mut [u8]) -> PollIoRes<(usize, SocketAddr)> {
        // Wait to be readable.
        ready!(self.0.poll_readable(cx)?);

        // Read from the socket.
        match self.0.get_ref().recv_from(buf) {
            Ok((n, addr)) => Poll::Ready(Ok((n, addr))),
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn poll_send_to(&self, cx: &mut Context, buf: &[u8], target: SocketAddr) -> PollIoRes<usize> {
        // Wait to be writable.
        ready!(self.0.poll_writable(cx)?);

        // Write to the socket.
        match self.0.get_ref().send_to(buf, target) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

#[pin_project]
#[derive(Debug)]
struct TcpStream(#[pin] Async<net::TcpStream>);

impl AsyncRead for TcpStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut [u8]) -> Poll<IoResult<usize>> {
        self.project().0.poll_read(cx, buf)
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<IoResult<usize>> {
        self.project().0.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<IoResult<()>> {
        self.project().0.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<IoResult<()>> {
        self.project().0.poll_close(cx)
    }
}

impl tcp::DnsTcpStream for TcpStream {
    type Time = DnsTime;
}

#[derive(Debug, Clone, Copy)]
struct DnsTime;

#[async_trait]
impl Time for DnsTime {
    async fn delay_for(dur: Duration) {
        Delay::new(dur).await;
    }

    async fn timeout<F>(dur: Duration, fut: F) -> IoResult<F::Output>
    where
        F: Future + Send + 'static,
    {
        fut.with_timeout(dur).map_err(|_| TimedOut.into()).await
    }
}

#[derive(Debug, Clone)]
struct HickoryHandle(DynSpawner);

impl Spawn for HickoryHandle {
    fn spawn_bg<F>(&mut self, fut: F)
    where
        F: Future<Output = Result<(), ProtoError>> + Send + 'static,
    {
        self.0.spawn_any(fut);
    }
}

#[derive(Debug, Clone)]
struct HickoryRuntimeProvider(DynSpawner);

impl RuntimeProvider for HickoryRuntimeProvider {
    type Handle = HickoryHandle;
    type Timer = DnsTime;
    type Udp = UdpSocket;
    type Tcp = TcpStream;

    fn create_handle(&self) -> Self::Handle {
        HickoryHandle(self.0.clone())
    }

    fn connect_tcp(
        &self,
        server_addr: SocketAddr,
        _: Option<SocketAddr>,
        _: Option<Duration>,
    ) -> Pin<Box<dyn Send + Future<Output = IoResult<Self::Tcp>>>> {
        Box::pin(async move {
            let stream = Async::<net::TcpStream>::connect(server_addr).await?;

            Ok(TcpStream(stream))
        })
    }

    fn bind_udp(
        &self,
        local_addr: SocketAddr,
        _: SocketAddr,
    ) -> Pin<Box<dyn Send + Future<Output = IoResult<Self::Udp>>>> {
        Box::pin(async move {
            let stream = Async::<net::UdpSocket>::bind(local_addr)?;

            Ok(UdpSocket(stream))
        })
    }
}

mod errors {
    use super::*;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error("DNS-over-UDP: {0}")]
    pub enum DnsClientError {
        Proto(#[from] ProtoError),
        Timeout(#[from] Timeout),
        Inner(#[from] Error),
    }

    impl From<DnsClientError> for Error {
        fn from(err: DnsClientError) -> Self {
            if let DnsClientError::Inner(err) = err {
                err.map_kind(ErrorKind::Resolve)
            } else {
                ErrorKind::resolve(err)
            }
        }
    }
}

use self::errors::*;
