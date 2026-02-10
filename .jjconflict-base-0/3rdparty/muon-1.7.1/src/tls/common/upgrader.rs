use crate::common::prelude::*;
use crate::tls::Alpn;
use crate::{Result, Sealed};
use async_trait::async_trait;
use muon_proc::{autoimpl, derive_dyn};
use std::sync::Arc;

/// A type that can upgrade a connection to use TLS.
#[async_trait]
#[autoimpl(for(DynTlsUpgrader))]
#[derive_dyn(Debug)]
pub trait TlsUpgrader: Sealed + Send + Sync + 'static {
    /// Upgrade the given socket to use TLS.
    async fn upgrade(
        &self,
        sock: DynSocket,
        host: &Host,
        name: &Name,
        alpn: &[Alpn],
    ) -> Result<(DynSocket, Option<Alpn>)>;
}

/// A dynamic upgrader; the underlying type is erased.
pub type DynTlsUpgrader = Arc<dyn TlsUpgrader>;

impl<This: TlsUpgrader> IntoDyn<DynTlsUpgrader> for This {
    fn into_dyn(self) -> DynTlsUpgrader {
        Arc::new(self)
    }
}

impl IntoDyn<DynTlsUpgrader> for &DynTlsUpgrader {
    fn into_dyn(self) -> DynTlsUpgrader {
        self.to_owned()
    }
}

if_sealed! {
    impl Sealed for DynTlsUpgrader {}
}
