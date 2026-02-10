//! ## Resolver
//!
//! This module defines the [`Resolver`] trait and related types.

use crate::common::{Addr, Host, IntoDyn};
use crate::{ErrorKind, Result};
use async_trait::async_trait;
use itertools::chain;
use muon_proc::{autoimpl, derive_dyn};
use std::collections::HashSet;
use std::iter::once;
use std::sync::Arc;
use thiserror::Error;

/// An error indicating that no addresses could be resolved for a host.
#[derive(Debug, Error)]
#[error("no addresses could be resolved for host")]
pub struct ResolveErr;

/// The result of resolving a host.
///
/// If a resolver is unable to resolve a host, for example because it does not
/// support indirect resolution, it can return a [`ResolveRes::None`] result.
/// This is not considered an error, and the client will attempt to resolve the
/// host using the next resolver in the chain.
#[derive(Debug)]
pub enum ResolveRes {
    /// The host was resolved to one or more addresses.
    Some(Addr, Vec<Addr>),

    /// The host should be resolved by another resolver.
    None,
}

impl ResolveRes {
    /// Consumes the result, converting it to a vector of resolved addresses.
    #[must_use]
    pub fn into_set(self) -> HashSet<Addr> {
        if let ResolveRes::Some(head, tail) = self {
            chain!(once(head), tail).collect()
        } else {
            HashSet::new()
        }
    }

    /// Consumes the result, converting it to `Result<HashSet<Addr>>`.
    ///
    /// This is a convenience method to help with type inference.
    ///
    /// # Errors
    ///
    /// Returns an error if no addresses were resolved.
    pub fn into_res(self) -> Result<HashSet<Addr>> {
        self.into()
    }
}

impl From<ResolveRes> for Result<HashSet<Addr>> {
    fn from(res: ResolveRes) -> Self {
        match res.into_set() {
            addr if !addr.is_empty() => Ok(addr),
            _ => Err(ErrorKind::resolve(ResolveErr)),
        }
    }
}

/// A type capable of resolving routes.
#[async_trait]
#[autoimpl(for(DynResolver))]
#[derive_dyn(Debug)]
pub trait Resolver: Send + Sync + 'static {
    /// Resolve the given host to a set of addresses.
    async fn resolve(&self, host: &Host) -> Result<ResolveRes>;
}

/// A dynamic resolver.
pub type DynResolver = Arc<dyn Resolver>;

impl<This: Resolver> IntoDyn<DynResolver> for This {
    fn into_dyn(self) -> DynResolver {
        Arc::new(self)
    }
}

impl IntoDyn<DynResolver> for &DynResolver {
    fn into_dyn(self) -> DynResolver {
        self.to_owned()
    }
}

/// An extension trait for the `Resolver` trait.
#[autoimpl]
pub trait ResolverExt: Resolver + Sized {
    /// Add a layer to the resolver.
    fn layer<L>(self, layer: impl IntoIterator<Item = L>) -> DynResolver
    where
        L: IntoDyn<DynResolverLayer>,
    {
        let this = self.into_dyn();

        (layer.into_iter())
            .fold(this, |r, l| (r, l.into_dyn()).into_dyn())
            .into_dyn()
    }
}

/// A resolver layer.
#[async_trait]
#[autoimpl(for(DynResolverLayer))]
#[derive_dyn(Debug)]
pub trait ResolverLayer: Send + Sync + 'static {
    /// Resolve the given host using the inner resolver.
    async fn on_resolve(&self, inner: &dyn Resolver, host: &Host) -> Result<ResolveRes>;
}

/// A dynamic resolver layer.
pub type DynResolverLayer = Arc<dyn ResolverLayer>;

impl<This: ResolverLayer> IntoDyn<DynResolverLayer> for This {
    fn into_dyn(self) -> DynResolverLayer {
        Arc::new(self)
    }
}

impl IntoDyn<DynResolverLayer> for &DynResolverLayer {
    fn into_dyn(self) -> DynResolverLayer {
        self.to_owned()
    }
}

/// Create a layer that sets another resolver as the fallback.
#[must_use]
pub fn with_fallback(resolver: impl IntoDyn<DynResolver>) -> DynResolverLayer {
    FallbackLayer(resolver.into_dyn()).into_dyn()
}

/// A layer that sets another resolver as the fallback.
#[derive(Debug)]
struct FallbackLayer(DynResolver);

#[async_trait]
impl ResolverLayer for FallbackLayer {
    async fn on_resolve(&self, inner: &dyn Resolver, host: &Host) -> Result<ResolveRes> {
        match inner.resolve(host).await {
            Ok(ResolveRes::Some(head, tail)) => {
                trace!("inner resolver succeeded, not using fallback resolver");
                Ok(ResolveRes::Some(head, tail))
            }

            Ok(ResolveRes::None) => {
                warn!("outer resolver returned no addresses, trying fallback resolver");
                self.0.resolve(host).await
            }

            Err(err) => {
                error!(%err, "outer resolver failed, trying fallback resolver");
                self.0.resolve(host).await
            }
        }
    }
}

#[async_trait]
impl<R, L> Resolver for (R, L)
where
    R: Resolver,
    L: ResolverLayer,
{
    async fn resolve(&self, host: &Host) -> Result<ResolveRes> {
        self.1.on_resolve(&self.0, host).await
    }
}
