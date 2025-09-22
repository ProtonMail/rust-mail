use crate::common::prelude::*;
use crate::tls::{
    Alpn, DynTrustAnchor, DynVerifier, ParseCert, TlsUpgrader, TokioAdapter, VerifyRes,
};
use crate::{ErrorKind, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::pki_types::{CertificateDer, InvalidDnsNameError, ServerName, UnixTime};
use tokio_rustls::rustls::CertificateError::*;
use tokio_rustls::rustls::{crypto, DigitallySignedStruct, Error, OtherError, SignatureScheme};
use tokio_rustls::{rustls, TlsConnector};
/// A Tokio TLS Upgrader that uses rustls
#[derive(Debug)]
pub struct TokioUpgrader {
    /// the certificate stores populated from the system and and roots from a
    /// [`DynTrustAnchor`].
    store: Arc<rustls::RootCertStore>,
    /// The [`CryptoProvider`] we use (it is ring)
    provider: Arc<CryptoProvider>,
    /// Our own way to verify the server certificate
    verifier: DynVerifier,
}

impl TokioUpgrader {
    /// Create a new rustls-based upgrader from the given verifier and anchors.
    /// Try to fetch the certificates from the system to populate the inner root
    /// cert store and extend it with the roots from the `anchor`.
    ///
    /// # Errors
    ///
    /// Returns an error if the upgrader could not be created.
    pub fn new(anchor: DynTrustAnchor, verifier: DynVerifier) -> Result<Self, TokioUpgraderErr> {
        let mut store = rustls::RootCertStore::empty();

        if let Ok(certs) = rustls_native_certs::load_native_certs() {
            debug!("load {} certificates from system", certs.len());
            let _ = store.add_parsable_certificates(certs);
        } else {
            warn!("can't load system certificates");
        }

        let certs = anchor
            .roots()
            .into_iter()
            .map(|v| CertificateDer::from_slice(v.as_slice()))
            .collect::<Vec<_>>();

        debug!("root contains {} certificates", certs.len());
        let _ = store.add_parsable_certificates(certs);

        Ok(Self {
            store: Arc::new(store),
            verifier,
            provider: Arc::new(crypto::ring::default_provider()),
        })
    }

    async fn upgrade(
        &self,
        sock: DynSocket,
        host: &Host,
        name: &Name,
        alpn: &[Alpn],
    ) -> Result<(DynSocket, Option<Alpn>), TokioUpgraderErr> {
        // try to create a webpki server verifier from the store, it may not work if
        // there is no roots
        let ver = rustls::client::WebPkiServerVerifier::builder_with_provider(
            self.store.clone(),
            self.provider.clone(),
        )
        .build()
        .ok();

        // build our own verifier that use our custom verifier first and then delegate
        // to (maybe) a webpki verifier
        let ver = Arc::new(RustlsVerifier {
            host: host.to_owned(),
            pki: ver,
            supported_schemes: self
                .provider
                .signature_verification_algorithms
                .supported_schemes(),
            ver: self.verifier.clone(),
        });

        // build the config, dangerous because we have our own verifier that is doing
        // the pinning (or something else ...)
        let mut config = rustls::ClientConfig::builder_with_provider(self.provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(ErrorKind::tls)?
            .dangerous()
            .with_custom_certificate_verifier(ver)
            .with_no_client_auth();

        // set the alpn protocols in the config
        config
            .alpn_protocols
            .extend(alpn.iter().map(|&s| s.to_vec()));

        let connector = TlsConnector::from(Arc::new(config));

        // during the connection, we will verify the server through our custom verifier
        let sock = (connector)
            .connect(name.to_string().try_into()?, TokioAdapter::new(sock))
            .await?;
        // if we were able to connect, get the alpn
        let alpn = sock
            .get_ref()
            .1
            .alpn_protocol()
            .and_then(|want| alpn.iter().find(|alpn| alpn.as_ref() == want).copied());
        Ok((TokioAdapter::new(sock).into_dyn(), alpn))
    }
}

#[async_trait]
impl TlsUpgrader for TokioUpgrader {
    async fn upgrade(
        &self,
        sock: DynSocket,
        host: &Host,
        name: &Name,
        alpn: &[Alpn],
    ) -> Result<(DynSocket, Option<Alpn>)> {
        trace!("performing TLS handshake with {name}");

        let sock = self.upgrade(sock, host, name, alpn).await?;

        trace!("TLS handshake with {name} complete");

        Ok(sock)
    }
}

#[derive(Debug)]
struct RustlsVerifier<Ver: ServerCertVerifier> {
    /// The host we are connecting to.
    host: Host,

    /// Maybe a verifier, none if no loaded certificates
    pki: Option<Arc<Ver>>,

    /// The supported scheme, used in case we don't have a
    /// [`ServerCertVerifier`]
    supported_schemes: Vec<SignatureScheme>,

    /// Our own verifier, e.g. for pinning.
    ver: DynVerifier,
}

impl<Ver: ServerCertVerifier> RustlsVerifier<Ver> {
    /// Just parse the cert and verify them using our own verifier
    /// ([`DynVerifier`])
    fn verify(&self, leaf: &CertificateDer, rest: &[CertificateDer]) -> Result<VerifyRes, Error> {
        let leaf = leaf.parse_der().map_err(|_| BadEncoding)?;
        let mut certs = Vec::with_capacity(rest.len());
        for cert in rest {
            let der = cert.parse_der().map_err(|_| BadEncoding)?;
            certs.push(der);
        }

        (self.ver)
            .verify(&self.host, &leaf, &certs)
            .map_err(|e| rustls::Error::Other(OtherError(Arc::new(e))))
    }
}

impl<Ver: ServerCertVerifier> ServerCertVerifier for RustlsVerifier<Ver> {
    fn verify_server_cert(
        &self,
        leaf: &CertificateDer,
        rest: &[CertificateDer],
        name: &ServerName,
        ocsp: &[u8],
        time: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // First, try to verify against our custom verifier
        match self.verify(leaf, rest)? {
            // If it is accepted, so be it
            VerifyRes::Accept => Ok(ServerCertVerified::assertion()),
            // If it is rejected, so be it
            VerifyRes::Reject => Err(rustls::CertificateError::ApplicationVerificationFailure)?,
            // If we had no answer, then proceed by trying to use the `pki`
            VerifyRes::Delegate => {
                // if we have one, cool, lets use it
                if let Some(pki) = self.pki.as_ref() {
                    pki.verify_server_cert(leaf, rest, name, ocsp, time)
                } else {
                    // Otherwise, it's rejected
                    Err(rustls::CertificateError::ApplicationVerificationFailure)?
                }
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        // First, try to verify against our custom verifier
        match self.verify(cert, &[])? {
            // If it is accepted, so be it
            VerifyRes::Accept => Ok(HandshakeSignatureValid::assertion()),
            // If it is rejected, so be it
            VerifyRes::Reject => Err(rustls::CertificateError::ApplicationVerificationFailure)?,
            // If we had no answer, then proceed by trying to use the `pki`
            VerifyRes::Delegate => {
                // if we have one, cool, lets use it
                if let Some(pki) = self.pki.as_ref() {
                    pki.verify_tls12_signature(msg, cert, dss)
                } else {
                    // Otherwise, it's rejected
                    Err(rustls::CertificateError::ApplicationVerificationFailure)?
                }
            }
        }
    }

    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        // First, try to verify against our custom verifier
        match self.verify(cert, &[])? {
            // If it is accepted, so be it
            VerifyRes::Accept => Ok(HandshakeSignatureValid::assertion()),
            // If it is rejected, so be it
            VerifyRes::Reject => Err(rustls::CertificateError::ApplicationVerificationFailure)?,
            // If we had no answer, then proceed by trying to use the `pki`
            VerifyRes::Delegate => {
                // if we have one, cool, lets use it
                if let Some(pki) = self.pki.as_ref() {
                    pki.verify_tls13_signature(msg, cert, dss)
                } else {
                    // Otherwise, it's rejected
                    Err(rustls::CertificateError::ApplicationVerificationFailure)?
                }
            }
        }
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported_schemes.clone()
    }
}

if_sealed! {
    impl crate::Sealed for TokioUpgrader {}
}

mod errors {
    use super::*;
    use crate::tls::ParseCertErr;
    use crate::Error;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error("invalid root certificate")]
    pub struct InvalidRootCertErr;

    #[derive(Debug, Error)]
    #[error("no peer certificate")]
    pub struct NoPeerCertErr;

    #[derive(Debug, Error)]
    #[error("certificate verification failed")]
    pub struct VerifyErr;

    #[derive(Debug, Error)]
    #[error("tokio upgrader: {0}")]
    pub enum TokioUpgraderErr {
        Rustls(#[from] InvalidDnsNameError),
        InvalidRootCert(#[from] InvalidRootCertErr),
        NoPeerCert(#[from] NoPeerCertErr),
        ParseCert(#[from] ParseCertErr),
        VerifyErr(#[from] VerifyErr),
        IO(#[from] std::io::Error),
        Inner(#[from] Error),
    }

    impl From<TokioUpgraderErr> for Error {
        fn from(err: TokioUpgraderErr) -> Self {
            if let TokioUpgraderErr::Inner(err) = err {
                err.map_kind(ErrorKind::Tls)
            } else {
                ErrorKind::tls(err)
            }
        }
    }
}

use self::errors::*;
