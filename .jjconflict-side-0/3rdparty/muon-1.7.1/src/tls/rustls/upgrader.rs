use crate::common::prelude::*;
use crate::tls::{Alpn, DynVerifier, ParseCert, TlsCert, TlsUpgrader, VerifyRes};
use crate::util::{IntoIterExt, ResultExt};
use crate::{ErrorKind, Result};
use async_trait::async_trait;
use futures_rustls::pki_types::{CertificateDer, InvalidDnsNameError, ServerName, UnixTime};
use futures_rustls::rustls::crypto;
use futures_rustls::{client, rustls, TlsConnector, TlsStream};
use muon_proc::autoimpl;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::CertificateError::{ApplicationVerificationFailure, BadEncoding};
use rustls::{ClientConfig, DigitallySignedStruct, Error, OtherError, SignatureScheme};
use rustls_platform_verifier::Verifier;
use std::borrow::{Borrow, BorrowMut, ToOwned};
use std::fmt::Debug;
use std::sync::Arc;

/// A rustls-based TLS upgrader.
#[derive(Debug)]
pub struct RustlsTlsUpgrader {
    pki: Arc<Verifier>,
    ver: DynVerifier,
}

impl RustlsTlsUpgrader {
    /// Create a new rustls-based upgrader from the given verifier and anchors.
    #[must_use]
    pub fn new(pki: Arc<Verifier>, ver: DynVerifier) -> Self {
        Self { pki, ver }
    }
}

#[async_trait]
impl TlsUpgrader for RustlsTlsUpgrader {
    async fn upgrade(
        &self,
        sock: DynSocket,
        host: &Host,
        name: &Name,
        alpn: &[Alpn],
    ) -> Result<(DynSocket, Option<Alpn>)> {
        trace!("building ring crypto provider for {name}");
        let ring = crypto::ring::default_provider();

        trace!("building TLS verifier for {name}");
        let ver = Arc::new(RustlsVerifier {
            host: host.to_owned(),
            pki: self.pki.clone(),
            ver: self.ver.clone(),
        });

        trace!("building TLS connector for {name}");
        let conn = ClientConfig::builder_with_provider(ring.into())
            .with_safe_default_protocol_versions()
            .map_err(ErrorKind::tls)?
            .dangerous()
            .with_custom_certificate_verifier(ver)
            .with_no_client_auth()
            .with_alpn_protocols(alpn)
            .into_tls_connector();

        trace!("performing TLS handshake with {name:?}");
        let name = name.to_server_name().map_err(ErrorKind::tls)?;
        let sock = conn.connect(name, sock).await.map_err(ErrorKind::tls)?;
        let alpn = sock.find_alpn(alpn);

        Ok((TlsStream::Client(sock).into_dyn(), alpn))
    }
}

if_sealed! {
    impl crate::Sealed for RustlsTlsUpgrader {}
}

#[derive(Debug)]
struct RustlsVerifier {
    /// The host we are connecting to.
    host: Host,

    // The webpki verifier, loaded with the root certificates.
    pki: Arc<Verifier>,

    // Our own verifier, e.g. for pinning.
    ver: DynVerifier,
}

impl RustlsVerifier {
    fn verify(&self, leaf: &CertificateDer, rest: &[CertificateDer]) -> Result<VerifyRes, Error> {
        let leaf = parse_der(leaf)?;
        let rest = rest.iter().map(parse_der).try_into_vec()?;

        Ok((self.ver)
            .verify(&self.host, &leaf, &rest)
            .map_err(|e| OtherError(Arc::new(e)))?)
    }
}

impl ServerCertVerifier for RustlsVerifier {
    fn verify_server_cert(
        &self,
        leaf: &CertificateDer,
        rest: &[CertificateDer],
        name: &ServerName,
        ocsp: &[u8],
        time: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        match self.verify(leaf, rest)? {
            VerifyRes::Accept => Ok(ServerCertVerified::assertion()),
            VerifyRes::Reject => Err(ApplicationVerificationFailure)?,
            VerifyRes::Delegate => self.pki.verify_server_cert(leaf, rest, name, ocsp, time),
        }
    }

    fn verify_tls12_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        match self.verify(cert, &[])? {
            VerifyRes::Accept => Ok(HandshakeSignatureValid::assertion()),
            VerifyRes::Reject => Err(ApplicationVerificationFailure)?,
            VerifyRes::Delegate => self.pki.verify_tls12_signature(msg, cert, dss),
        }
    }

    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        match self.verify(cert, &[])? {
            VerifyRes::Accept => Ok(HandshakeSignatureValid::assertion()),
            VerifyRes::Reject => Err(ApplicationVerificationFailure)?,
            VerifyRes::Delegate => self.pki.verify_tls13_signature(msg, cert, dss),
        }
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.pki.supported_verify_schemes()
    }
}

fn parse_der<'a>(cert: &'a CertificateDer<'a>) -> Result<TlsCert<'a>, Error> {
    cert.parse_der().map_err(|_| BadEncoding).err_into()
}

#[autoimpl]
trait ToServerName: AsRef<str> {
    fn to_server_name(&self) -> Result<ServerName<'static>, InvalidDnsNameError> {
        self.as_ref().to_owned().try_into()
    }
}

#[autoimpl]
trait ClientConfigExt: Sized {
    fn with_alpn_protocols(mut self, alpn: &[Alpn]) -> Self
    where
        Self: BorrowMut<ClientConfig>,
    {
        let alpn = alpn.iter().map(|&s| s.to_vec());

        self.borrow_mut().alpn_protocols.extend(alpn);

        self
    }

    fn into_tls_connector(self) -> TlsConnector
    where
        Self: Into<ClientConfig>,
    {
        TlsConnector::from(Arc::new(self.into()))
    }
}

#[autoimpl]
trait TlsStreamExt<'a, IO: 'a>: Borrow<client::TlsStream<IO>> {
    /// Returns the ALPN protocol used, if any.
    fn have_alpn(&'a self) -> Option<&'a [u8]> {
        self.borrow().get_ref().1.alpn_protocol()
    }

    /// Returns which of the given ALPN protocols is used, if any.
    fn find_alpn(&'a self, alpn: &[Alpn]) -> Option<Alpn> {
        let want = self.have_alpn()?;

        alpn.iter().find(|alpn| alpn.as_ref() == want).copied()
    }
}
