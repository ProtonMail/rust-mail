use crate::common::prelude::*;
use crate::tls::TlsCertDer;
use muon_proc::{autoimpl, derive_dyn};
use std::fmt::Debug;
use std::sync::Arc;

/// A trait for types that can provide trust anchors.
///
/// This type can provide a set of root certificates for verifying server
/// certificates.
#[autoimpl(for(DynTrustAnchor))]
#[derive_dyn(Debug)]
pub trait TrustAnchor: Send + Sync + 'static {
    /// Get the root certificates from this trust anchor.
    fn roots(&self) -> Vec<&TlsCertDer>;
}

/// A dynamic trust anchor; the underlying type is erased.
pub type DynTrustAnchor = Arc<dyn TrustAnchor>;

impl<This: TrustAnchor> IntoDyn<DynTrustAnchor> for This {
    fn into_dyn(self) -> DynTrustAnchor {
        Arc::new(self)
    }
}

impl IntoDyn<DynTrustAnchor> for &DynTrustAnchor {
    fn into_dyn(self) -> DynTrustAnchor {
        self.to_owned()
    }
}

/// An extension trait for the `TrustAnchor` trait.
#[autoimpl]
pub trait TrustAnchorExt: TrustAnchor + Sized {
    /// Extend this trust anchor with another trust anchor.
    ///
    /// This method is used to chain trust anchors together.
    /// The resulting trust anchor should provide the union of the root
    /// certificates from both trust anchors.
    fn chain<T>(self, other: impl IntoIterator<Item = T>) -> DynTrustAnchor
    where
        T: IntoDyn<DynTrustAnchor>,
    {
        let this = self.into_dyn();

        (other.into_iter())
            .fold(this, |a, o| (a, o.into_dyn()).into_dyn())
            .into_dyn()
    }
}

/// A base trust anchor that does not provide any root certificates.
#[derive(Debug)]
pub struct BaseTrustAnchor;

impl TrustAnchor for BaseTrustAnchor {
    fn roots(&self) -> Vec<&TlsCertDer> {
        Vec::new()
    }
}

impl<L: TrustAnchor, R: TrustAnchor> TrustAnchor for (L, R) {
    fn roots(&self) -> Vec<&TlsCertDer> {
        [self.0.roots(), self.1.roots()].concat()
    }
}
