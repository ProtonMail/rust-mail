use anyhow::Result;
use crate::tls::{TlsCertDer, TrustAnchor};
use rcgen::{BasicConstraints, Certificate, CertificateParams, IsCa, KeyPair};

/// A custom trust anchor.
#[derive(Debug)]
pub struct TestTrustAnchor(Vec<u8>);

impl TestTrustAnchor {
    /// Create a new test trust anchor from the given DER-encoded certificate.
    pub fn from_der(anchor: impl AsRef<[u8]>) -> Self {
        Self(anchor.as_ref().to_vec())
    }
}

impl TrustAnchor for TestTrustAnchor {
    fn roots(&self) -> Vec<&TlsCertDer> {
        vec![&self.0]
    }
}

/// Generates a self-signed certificate authority (CA) and key pair.
pub fn generate_ca() -> Result<(Certificate, KeyPair)> {
    let kp = KeyPair::generate()?;

    Ok((new_ca_params()?.self_signed(&kp)?, kp))
}

/// Generate a new self-signed certificate and key pair, signed by the given CA.
pub fn generate_cert(ca: &Certificate, key: &KeyPair) -> Result<(Certificate, KeyPair)> {
    let kp = KeyPair::generate()?;

    Ok((new_cert_params()?.signed_by(&kp, ca, key)?, kp))
}

fn new_ca_params() -> Result<CertificateParams> {
    let mut params = CertificateParams::new([])?;

    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);

    Ok(params)
}

fn new_cert_params() -> Result<CertificateParams> {
    let mut params = CertificateParams::new([])?;

    params.is_ca = IsCa::ExplicitNoCa;

    Ok(params)
}
