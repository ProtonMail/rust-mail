use crate::common::prelude::*;
use crate::tls::*;
use crate::{ErrorKind, Result};
use futures_rustls::rustls::{crypto, Error};
use rustls_platform_verifier::Verifier;
use std::sync::Arc;

/// A TLS implementation using the `rustls` crate.
#[derive(Debug)]
pub struct RustlsTls;

impl Tls for RustlsTls {
    fn build(&self, anchor: DynTrustAnchor, verifier: DynVerifier) -> Result<DynTlsUpgrader> {
        let pki = new_pki_verifier(&anchor.roots()).map_err(ErrorKind::tls)?;

        Ok(RustlsTlsUpgrader::new(Arc::new(pki), verifier).into_dyn())
    }
}

if_sealed! {
    impl crate::Sealed for RustlsTls {}
}

/// Create a new `rustls`-based verifier from the given roots.
///
/// This only works on linux.
#[cfg(target_os = "linux")]
fn new_pki_verifier(roots: &[&TlsCertDer]) -> Result<Verifier, Error> {
    use futures_rustls::pki_types::CertificateDer;

    let mut anchors = Vec::new();

    for root in roots.iter().map(|root| CertificateDer::from_slice(root)) {
        anchors.push(root.into_owned());
    }

    Verifier::new_with_extra_roots(anchors, crypto::ring::default_provider().into())
}

/// Create a new `rustls`-based verifier.
#[cfg(not(target_os = "linux"))]
fn new_pki_verifier(_: &[&TlsCertDer]) -> Result<Verifier, Error> {
    Verifier::new(crypto::ring::default_provider().into())
}
