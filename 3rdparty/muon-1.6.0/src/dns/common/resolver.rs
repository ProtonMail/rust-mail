use crate::common::prelude::*;
use crate::rt::{ResolveRes, Resolver};
use crate::util::{ByteSliceExt, IntoIterExt};
use crate::{Error, ErrorKind, Result};
use async_trait::async_trait;
use hickory_proto::op::{Message, Query, ResponseCode};
use hickory_proto::rr::rdata::{A, AAAA, TXT};
use hickory_proto::rr::{RData, Record, RecordType};
use hickory_proto::ProtoError;
use itertools::Itertools;
use muon_proc::autoimpl;
use std::sync::Arc;
use thiserror::Error;

/// An abstract DNS client.
///
/// This type is able to communicate with a DNS server and perform DNS queries.
#[async_trait]
#[autoimpl(for(DynDns))]
pub trait Dns: Send + Sync + 'static {
    /// Perform a DNS query, returning the response message.
    async fn query(&self, msg: Message) -> Result<Vec<Message>>;
}

/// A dynamic abstract DNS client.
pub type DynDns = Arc<dyn Dns>;

impl<This: Dns> IntoDyn<DynDns> for This {
    fn into_dyn(self) -> DynDns {
        Arc::new(self)
    }
}

impl IntoDyn<DynDns> for &DynDns {
    fn into_dyn(self) -> DynDns {
        self.to_owned()
    }
}

/// Extension methods for the `Dns` trait.
#[autoimpl]
trait DnsExt: Dns {
    /// Resolve the given host to a list of `RData` records.
    ///
    /// # Errors
    ///
    /// Returns an error if the host cannot be resolved.
    async fn lookup(&self, name: &Name, rt: RecordType) -> Result<Vec<RData>, DnsLookupErr> {
        trace!(%name, %rt, "performing DNS lookup");

        let qry = Query::query(name.parse()?, rt);
        let msg = new_message(qry);
        let res = self.query(msg).await?;

        let mut ans = Vec::new();

        for msg in res {
            match msg.response_code() {
                ResponseCode::NoError => {
                    trace!(msg = %fmt_msg(&msg), "received DNS response");
                    ans.extend(get_answers(msg))
                }

                code => {
                    error!(%code, "DNS response error");
                    Err(ResponseErr::new(code))?
                }
            }
        }

        Ok(ans)
    }

    /// Resolve the given host to a set of IP addresses.
    ///
    /// This method resolves both IPv4 and IPv6 addresses.
    ///
    /// # Errors
    ///
    /// Returns an error if the host cannot be resolved.
    async fn lookup_addr(&self, name: &Name) -> Result<Vec<Addr>, DnsLookupErr> {
        trace!(%name, "looking up IP addresses");

        let mut res = Vec::new();

        for rd in self.lookup(name, RecordType::A).await? {
            match rd {
                RData::A(A(ip)) => res.push(Addr::new(name.to_owned(), ip.into())),
                other => debug!(?other, "unexpected DNS record"),
            }
        }

        for rd in self.lookup(name, RecordType::AAAA).await? {
            match rd {
                RData::AAAA(AAAA(ip)) => res.push(Addr::new(name.to_owned(), ip.into())),
                other => debug!(?other, "unexpected DNS record"),
            }
        }

        Ok(res)
    }

    /// Resolve the given host's TXT records as names.
    ///
    /// # Errors
    ///
    /// Returns an error if the host cannot be resolved.
    async fn lookup_name(&self, name: &Name) -> Result<Vec<Name>, DnsLookupErr> {
        trace!(%name, "looking up TXT records");

        let mut res = Vec::new();

        for rd in self.lookup(name, RecordType::TXT).await? {
            match rd {
                RData::TXT(txt) => res.extend(txt.to_names()),
                other => debug!(?other, "unexpected DNS record"),
            }
        }

        Ok(res)
    }
}

/// A DNS resolver.
///
/// This type wraps a DNS client to implement the [`Resolver`] trait.
#[derive(Debug)]
pub struct DnsResolver<D>(D);

impl<D: Dns> DnsResolver<D> {
    /// Create a new resolver with the given DNS client.
    #[must_use]
    pub fn new(client: D) -> Self {
        Self(client)
    }

    async fn resolve_direct(&self, name: &Name) -> Result<ResolveRes> {
        trace!(%name, "resolving name via direct lookup");

        if let Some((head, tail)) = self.0.lookup_addr(name).await?.into_head_tail() {
            Ok(ResolveRes::Some(head, tail.collect()))
        } else {
            Ok(ResolveRes::None)
        }
    }

    async fn resolve_indirect(&self, name: &Name) -> Result<ResolveRes> {
        trace!(%name, "resolving name via indirect lookup");

        let mut res = Vec::new();

        for name in self.0.lookup_name(name).await? {
            res.extend(self.0.lookup_addr(&name).await?);
        }

        if let Some((head, tail)) = res.into_head_tail() {
            Ok(ResolveRes::Some(head, tail.collect()))
        } else {
            Ok(ResolveRes::None)
        }
    }
}

#[async_trait]
impl<D: Dns> Resolver for DnsResolver<D> {
    #[instrument(level = "debug", skip(self), fields(%host))]
    async fn resolve(&self, host: &Host) -> Result<ResolveRes> {
        trace!("resolving host");

        match host {
            Host::Direct(name) => self.resolve_direct(name).await,
            Host::Indirect(name) => self.resolve_indirect(name).await,
        }
    }
}

/// Format a DNS query for debugging.
pub(crate) fn fmt_msg(msg: &Message) -> String {
    let mut parts = Vec::new();

    match msg.queries().iter().map(|q| q.name()).join(", ") {
        s if !s.is_empty() => parts.push(format!("q: [{s}]")),
        _ => (),
    };

    match msg.answers().iter().map(|a| a.name()).join(", ") {
        s if !s.is_empty() => parts.push(format!("a: [{s}]")),
        _ => (),
    };

    parts.join(", ")
}

fn new_message(qry: Query) -> Message {
    Message::new()
        .add_query(qry)
        .set_recursion_desired(true)
        .to_owned()
}

fn get_answers(msg: Message) -> Vec<RData> {
    msg.into_parts()
        .answers
        .into_iter()
        .map(Record::into_data)
        .collect()
}

trait TxtExt {
    fn to_names(&self) -> Vec<Name>;
}

impl TxtExt for TXT {
    fn to_names(&self) -> Vec<Name> {
        let mut res = Vec::new();

        for txt in self.txt_data() {
            let Ok(txt) = txt.as_utf8() else { continue };
            let Ok(name) = txt.parse() else { continue };
            res.push(name);
        }

        res
    }
}

mod errors {
    use super::*;

    #[derive(Debug, Error)]
    #[error("non-zero DNS response code: {0}")]
    pub struct ResponseErr(ResponseCode);

    impl ResponseErr {
        pub fn new(code: ResponseCode) -> Self {
            Self(code)
        }
    }

    #[derive(Debug, Error)]
    #[error("DNS lookup: {0}")]
    pub enum DnsLookupErr {
        Response(#[from] ResponseErr),
        Proto(#[from] ProtoError),
        Inner(#[from] Error),
    }

    impl From<DnsLookupErr> for Error {
        fn from(err: DnsLookupErr) -> Self {
            if let DnsLookupErr::Inner(err) = err {
                err.map_kind(ErrorKind::Resolve)
            } else {
                ErrorKind::resolve(err)
            }
        }
    }
}

use self::errors::*;
