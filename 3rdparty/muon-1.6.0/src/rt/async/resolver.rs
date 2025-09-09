use crate::common::{Addr, Host, Name};
use crate::rt::{ResolveRes, Resolver};
use crate::util::IntoIterExt;
use crate::{ErrorKind, Result};
use async_trait::async_trait;
use futures::TryFutureExt;
use std::net::SocketAddr;

/// An async resolver.
#[derive(Debug)]
pub struct AsyncResolver;

impl AsyncResolver {
    async fn resolve_direct(&self, name: &Name) -> Result<ResolveRes> {
        let mut res = Vec::new();

        for addr in lookup(name.as_ref()).await? {
            res.push(Addr::new(name.to_owned(), addr.ip()));
        }

        if let Some((head, tail)) = res.into_head_tail() {
            Ok(ResolveRes::Some(head, tail.collect()))
        } else {
            Ok(ResolveRes::None)
        }
    }
}

async fn lookup(name: &str) -> Result<Vec<SocketAddr>> {
    async_net::resolve((name, 0))
        .map_err(ErrorKind::resolve)
        .await
}

#[async_trait]
impl Resolver for AsyncResolver {
    #[instrument(level = "debug", skip(self), fields(%host))]
    async fn resolve(&self, host: &Host) -> Result<ResolveRes> {
        trace!("resolving host");

        if let Host::Direct(name) = host {
            trace!(%name, "resolving hostname");
            self.resolve_direct(name).await
        } else {
            trace!("indirect resolution not supported");
            Ok(ResolveRes::None)
        }
    }
}
