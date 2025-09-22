use crate::common::prelude::*;
use crate::common::IntoDyn;
use crate::dns::{fmt_msg, impl_doh_service, Dns, DnsResolver, DynDohService};
use crate::http::{Accept, DynBoundHttpConnector, DynHttpConnector, HttpReqExt, GET};
use crate::rt::DynResolver;
use crate::util::ByteSliceExt;
use crate::{Error, ErrorKind, Result};
use async_trait::async_trait;
use derive_more::Display;
use hickory_proto::op::Message;
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use hickory_proto::ProtoError;
use std::borrow::Borrow;

/// A DNS client that uses a public DNS-over-HTTPS server.
#[derive(Debug)]
pub struct DohClient(DynBoundHttpConnector);

impl DohClient {
    /// Create a new DNS-over-HTTPS client that uses the given server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server URL is invalid.
    pub fn new<D, C>(service: D, connector: C) -> Self
    where
        D: IntoDyn<DynDohService>,
        C: Borrow<DynHttpConnector>,
    {
        let host = service.into_dyn().server();
        let conn = connector.borrow().to_owned();

        DohClient(conn.bind(host))
    }

    async fn query(&self, msg: Message) -> Result<Vec<Message>, DohClientErr> {
        trace!("connecting to DNS-over-HTTPS server");
        let sender = self.0.connect().await?;

        trace!("sending {msg} over HTTPS");
        let res = GET!("/dns-query")
            .query(("dns", msg.to_bytes()?.as_b64_url()))
            .header(Accept::DNS)
            .send_with(&sender)
            .with_timeout(DNS_DOH_QUERY)
            .await??;

        trace!("received response {res}");
        Ok(vec![Message::from_bytes(res.body())?])
    }
}

#[async_trait]
impl Dns for DohClient {
    #[instrument(level = "debug", skip(self), fields(msg = %fmt_msg(&msg)))]
    async fn query(&self, msg: Message) -> Result<Vec<Message>> {
        trace!("performing DNS-over-HTTPS query");

        match self.query(msg).await {
            Ok(res) => {
                trace!("received DNS-over-HTTPS response");
                Ok(res)
            }

            Err(err) => {
                error!(%err, "failed to perform DNS-over-HTTPS query");
                Err(err)?
            }
        }
    }
}

impl IntoDyn<DynResolver> for DohClient {
    fn into_dyn(self) -> DynResolver {
        DnsResolver::new(self).into_dyn()
    }
}

/// The Google public DNS-over-HTTPS service.
#[derive(Debug, Display)]
pub struct GoogleDoh;

/// The Cloudflare public DNS-over-HTTPS service.
#[derive(Debug, Display)]
pub struct CloudflareDoh;

/// The Quad9 public DNS-over-HTTPS service.
#[derive(Debug, Display)]
pub struct Quad9Doh;

// Implement the DNS-over-HTTPS service trait.
impl_doh_service! {
    CloudflareDoh => "https://cloudflare-dns.com",
    GoogleDoh => "https://dns.google",
    Quad9Doh => "https://dns.quad9.net",
}

mod errors {
    use super::*;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error("DNS-over-HTTPS: {0}")]
    pub enum DohClientErr {
        Proto(#[from] ProtoError),
        Timeout(#[from] Timeout),
        Inner(#[from] Error),
    }

    impl From<DohClientErr> for Error {
        fn from(err: DohClientErr) -> Self {
            if let DohClientErr::Inner(err) = err {
                err.map_kind(ErrorKind::Resolve)
            } else {
                ErrorKind::resolve(err)
            }
        }
    }
}

use self::errors::*;
