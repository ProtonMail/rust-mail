use crate::common::prelude::*;
use crate::tls::TlsCert;
use crate::tls::VerifyRes::*;
use crate::Result;
use muon_proc::{autoimpl, derive_dyn};
use std::fmt::Debug;
use std::sync::Arc;

/// A trait for types that can verify a server's certificate(s).
#[autoimpl(for(DynVerifier))]
#[derive_dyn(Debug)]
pub trait Verifier: Send + Sync + 'static {
    /// Verify the server's certificate.
    ///
    /// If this verifier can determine that the certificate is valid or invalid,
    /// it should return `Some(VerifyRes::Accept)` or `Some(VerifyRes::Reject)`
    /// respectively. If the verifier cannot determine the validity of the
    /// certificate, it should return `None`.
    ///
    /// # Errors
    ///
    /// Returns an error if the verification process fails.
    fn verify(&self, host: &Host, head: &TlsCert, tail: &[TlsCert]) -> Result<VerifyRes>;
}

/// A dynamic verifier; the underlying type is erased.
pub type DynVerifier = Arc<dyn Verifier>;

impl<This: Verifier> IntoDyn<DynVerifier> for This {
    fn into_dyn(self) -> DynVerifier {
        Arc::new(self)
    }
}

impl IntoDyn<DynVerifier> for &DynVerifier {
    fn into_dyn(self) -> DynVerifier {
        self.to_owned()
    }
}

/// An extension trait for the [`Verifier`] trait.
#[autoimpl]
pub trait VerifierExt: Verifier + Sized {
    /// Extend this verifier with another verifier.
    ///
    /// This method is used to chain verifiers together.
    /// If the first verifier returns `None`, the next verifier is called.
    fn chain<T>(self, other: impl IntoIterator<Item = T>) -> DynVerifier
    where
        T: IntoDyn<DynVerifier>,
    {
        let this = self.into_dyn();

        (other.into_iter())
            .fold(this, |v, o| (v, o.into_dyn()).into_dyn())
            .into_dyn()
    }
}

/// The result of a certificate verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyRes {
    /// The verifier accepts the certificate.
    Accept,

    /// The verifier rejects the certificate.
    Reject,

    /// The verifier delegates verification to another verifier.
    Delegate,
}

/// A base verifier that makes no verification decisions.
#[derive(Debug)]
pub struct BaseVerifier;

impl Verifier for BaseVerifier {
    fn verify(&self, _: &Host, _: &TlsCert, _: &[TlsCert]) -> Result<VerifyRes> {
        Ok(Delegate)
    }
}

impl<L: Verifier, R: Verifier> Verifier for (L, R) {
    fn verify(&self, host: &Host, head: &TlsCert, tail: &[TlsCert]) -> Result<VerifyRes> {
        trace!(?host, "verifying with LHS");
        if let res @ (Accept | Reject) = self.0.verify(host, head, tail)? {
            return Ok(res);
        }

        trace!(?host, "verifying with RHS");
        if let res @ (Accept | Reject) = self.1.verify(host, head, tail)? {
            return Ok(res);
        }

        trace!(?host, "neither LHS nor RHS made a decision");
        Ok(Delegate)
    }
}
