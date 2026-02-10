use crate::common::IntoDyn;
use crate::tls::{DynTlsUpgrader, DynTrustAnchor, DynVerifier};
use crate::{Result, Sealed};
use muon_proc::{autoimpl, derive_dyn};
use std::sync::Arc;

/// A TLS backend, used to create a TLS upgrader.
///
/// This trait is used to abstract over different TLS implementations.
/// Concrete implementations are enabled via feature flags.
#[autoimpl(for(DynTls))]
#[derive_dyn(Debug)]
pub trait Tls: Sealed + Send + Sync + 'static {
    /// Create an upgrader with the given dynamic trust anchor and verifier.
    ///
    /// # Errors
    ///
    /// Returns an error if the upgrader could not be created.
    fn build(&self, anchor: DynTrustAnchor, verifier: DynVerifier) -> Result<DynTlsUpgrader>;
}

/// A dynamic TLS factory; the underlying type is erased.
pub type DynTls = Arc<dyn Tls>;

impl<This: Tls> IntoDyn<DynTls> for This {
    fn into_dyn(self) -> DynTls {
        Arc::new(self)
    }
}

impl IntoDyn<DynTls> for &DynTls {
    fn into_dyn(self) -> DynTls {
        self.to_owned()
    }
}

if_sealed! {
    impl Sealed for DynTls {}
}

/// An extension trait for the `Tls` trait.
#[autoimpl]
pub trait TlsExt: Tls {
    /// Create an upgrader with the given trust anchor and cert verifier.
    ///
    /// # Errors
    ///
    /// Returns an error if the upgrader could not be created.
    fn build_any<A, V>(&self, anchor: A, verifier: V) -> Result<DynTlsUpgrader>
    where
        A: IntoDyn<DynTrustAnchor>,
        V: IntoDyn<DynVerifier>,
    {
        self.build(anchor.into_dyn(), verifier.into_dyn())
    }
}
