use crate::common::prelude::*;
use crate::tls::{
    Alpn, DynTrustAnchor, DynVerifier, ParseCert, TlsUpgrader, TokioAdapter, VerifyRes,
};
use crate::{ErrorKind, Result};
use async_trait::async_trait;
use tokio_native_tls::native_tls::Certificate;
use tokio_native_tls::{native_tls, TlsConnector};

/// A tokio-native-tls upgrader.
#[derive(Debug)]
pub struct TokioUpgrader {
    connector: TlsConnector,
    verifier: DynVerifier,
}

impl TokioUpgrader {
    /// Create a new rustls-based upgrader from the given verifier and anchors.
    ///
    /// # Errors
    ///
    /// Returns an error if the upgrader could not be created.
    pub fn new(anchor: DynTrustAnchor, verifier: DynVerifier) -> Result<Self, native_tls::Error> {
        let mut inner = native_tls::TlsConnector::builder();

        for root in anchor.roots() {
            inner.add_root_certificate(Certificate::from_der(root)?);
        }

        let connector = TlsConnector::from(inner.build()?);

        Ok(Self {
            connector,
            verifier,
        })
    }

    async fn upgrade(
        &self,
        sock: DynSocket,
        host: &Host,
        name: &Name,
        _: &[Alpn],
    ) -> Result<(DynSocket, Option<Alpn>), TokioUpgraderErr> {
        trace!("performing TLS handshake with {name}");

        let sock = (self.connector)
            .connect(name, TokioAdapter::new(sock))
            .await?;

        let cert = sock
            .get_ref()
            .peer_certificate()?
            .ok_or(NoPeerCertErr)?
            .to_der()?;

        if let VerifyRes::Reject = self.verifier.verify(host, &cert.parse_der()?, &[])? {
            Err(VerifyErr)?
        } else {
            Ok((TokioAdapter::new(sock).into_dyn(), None))
        }
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

if_sealed! {
    impl crate::Sealed for TokioUpgrader {}
}

mod errors {
    use super::*;
    use crate::tls::ParseCertErr;
    use crate::Error;
    use thiserror::Error;

    #[derive(Debug, Error)]
    #[error("no peer certificate")]
    pub struct NoPeerCertErr;

    #[derive(Debug, Error)]
    #[error("certificate verification failed")]
    pub struct VerifyErr;

    #[derive(Debug, Error)]
    #[error("tokio upgrader: {0}")]
    pub enum TokioUpgraderErr {
        NativeTls(#[from] native_tls::Error),
        NoPeerCert(#[from] NoPeerCertErr),
        ParseCert(#[from] ParseCertErr),
        VerifyErr(#[from] VerifyErr),
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
