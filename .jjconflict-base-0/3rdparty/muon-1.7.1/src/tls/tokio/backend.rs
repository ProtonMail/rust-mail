use crate::common::IntoDyn;
use crate::tls::{DynTlsUpgrader, DynTrustAnchor, DynVerifier, Tls, TokioUpgrader};
use crate::{ErrorKind, Result};

/// A TLS implementation using the `rustls` crate.
#[derive(Debug)]
pub struct TokioTls;

impl Tls for TokioTls {
    fn build(&self, anchor: DynTrustAnchor, verifier: DynVerifier) -> Result<DynTlsUpgrader> {
        Ok(TokioUpgrader::new(anchor, verifier)
            .map_err(ErrorKind::tls)?
            .into_dyn())
    }
}

if_sealed! {
    impl crate::Sealed for TokioTls {}
}
