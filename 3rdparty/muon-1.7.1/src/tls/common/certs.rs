use crate::Result;
use derive_more::AsRef;
use muon_proc::autoimpl;
use std::fmt::Debug;
use thiserror::Error;
use x509_parser::certificate::X509Certificate;
use x509_parser::error::{PEMError, X509Error};
use x509_parser::nom;
use x509_parser::pem::Pem;

/// A TLS certificate.
pub type TlsCert<'a> = X509Certificate<'a>;

/// A DER-encoded TLS certificate.
pub type TlsCertDer = Vec<u8>;

/// An error that can occur while parsing a certificate.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum ParseCertErr {
    /// An error occurred while parsing a DER-encoded certificate.
    Der(#[from] nom::Err<X509Error>),

    /// An error occurred while parsing a PEM-encoded certificate.
    Pem(#[from] nom::Err<PEMError>),
}

/// Parse an X.509 TLS certificate.
#[autoimpl]
pub trait ParseCert<'a>: AsRef<[u8]> {
    /// Parse a DER-encoded certificate.
    ///
    /// # Errors
    ///
    /// Returns an error if the certificate could not be parsed.
    fn parse_der(&'a self) -> Result<TlsCert<'a>, ParseCertErr> {
        Ok(x509_parser::parse_x509_certificate(self.as_ref()).map(|(_, cert)| cert)?)
    }

    /// Parse a PEM-encoded certificate.
    ///
    /// # Errors
    ///
    /// Returns an error if the certificate could not be parsed.
    fn parse_pem(&'a self) -> Result<Pem, ParseCertErr> {
        Ok(x509_parser::pem::parse_x509_pem(self.as_ref()).map(|(_, pem)| pem)?)
    }
}
