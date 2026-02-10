//! This module provides the [`Prod`] environment.

use crate::app::{AppVersion, Platform, Product};
use crate::common::prelude::*;
use crate::env::{Env, Server};
use crate::tls::TlsPinSet;

/// The production API environment.
///
/// This environment represents the production Proton API, used by default.
/// Clients using this environment will connect to `https://*.proton.me/`.
#[derive(Debug)]
pub struct Prod {
    direct: TlsPinSet,
    indirect: TlsPinSet,
}

impl Default for Prod {
    fn default() -> Self {
        let direct = TlsPinSet::from_b64([
            "CT56BhOTmj5ZIPgb/xD5mH8rY3BLo/MlhP7oPyJUEDo=",
            "35Dx28/uzN3LeltkCBQ8RHK0tlNSa2kCpCRGNp34Gxc=",
            "qYIukVc63DEITct8sFT7ebIq5qsWmuscaIKeJx+5J5A=",
        ])
        .unwrap();

        let indirect = TlsPinSet::from_b64([
            "EU6TS9MO0L/GsDHvVc9D5fChYLNy5JdGYpJw0ccgetM=",
            "iKPIHPnDNqdkvOnTClQ8zQAIKG0XavaPkcEo0LBAABA=",
            "MSlVrBCdL0hKyczvgYVSRNm88RicyY04Q2y5qrBt0xA=",
            "C2UxW0T1Ckl9s+8cXfjXxlEqwAfPM4HiW2y3UdtBeCw=",
        ])
        .unwrap();

        Self { direct, indirect }
    }
}

impl Env for Prod {
    fn servers(&self, version: &AppVersion) -> Vec<Server> {
        let (plat, prod) = if let Some(name) = version.name() {
            (name.platform(), name.product())
        } else {
            (&Platform::Web, &Product::Mail)
        };

        let (host, path) = if let Platform::Web = plat {
            (format!("{prod}.proton.me"), "/api")
        } else {
            (format!("{prod}-api.proton.me"), "/")
        };

        vec![
            Server::https(Host::direct(&host).unwrap(), path),
            Server::https(Host::direct(&host).unwrap().to_indirect(), path),
        ]
    }

    fn pins(&self, host: &Host) -> Option<&TlsPinSet> {
        match host {
            Host::Direct(name) => want_name(name).then_some(&self.direct),
            Host::Indirect(_) => Some(&self.indirect),
        }
    }
}

fn want_name(name: &Name) -> bool {
    name.as_ref() == "proton.me" || name.ends_with(".proton.me")
}

if_sealed! {
    impl crate::Sealed for Prod {}
}
