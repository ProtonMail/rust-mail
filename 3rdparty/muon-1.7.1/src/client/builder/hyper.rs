use crate::client::builder::{BaseBuilder, Transport};
use crate::common::prelude::*;
use crate::env::DynEnv;
use crate::http::*;
use crate::rt::*;
use crate::tls::*;
use crate::Result;

if_dns! {
    use crate::dns::*;
}

/// A [`BaseBuilder`] for configuring a [`Hyper`] transport.
pub type HyperBuilder = BaseBuilder<Hyper>;

impl HyperBuilder {
    /// Set the spawner for this client.
    pub fn spawner(mut self, spawner: impl Spawner) -> Self {
        self.inner.spawner = Some(spawner.into_dyn());
        self
    }

    /// Set the resolver for this client.
    pub fn resolver(mut self, resolver: impl Resolver) -> Self {
        self.inner.resolver = Some(resolver.into_dyn());
        self
    }

    /// Set the dialer for this client.
    pub fn dialer(mut self, dialer: impl Dialer) -> Self {
        self.inner.dialer = Some(dialer.into_dyn());
        self
    }

    /// Add a trust anchor to this client.
    pub fn anchor(mut self, anchor: impl TrustAnchor) -> Self {
        self.inner.anchor = self.inner.anchor.chain([anchor]);
        self
    }

    /// Add a verifier to this client.
    pub fn verifier(mut self, verifier: impl Verifier) -> Self {
        self.inner.verifier = self.inner.verifier.chain([verifier]);
        self
    }

    /// Set the TLS factory for this client.
    pub fn tls(mut self, tls: impl Tls) -> Self {
        self.inner.tls = Some(tls.into_dyn());
        self
    }

    if_dns_client! {
        /// Add DNS services to this client.
        pub fn dns<D: DnsService>(mut self, dns: impl IntoIterator<Item = D>) -> Self {
            let dns = dns.into_iter().map(IntoDyn::into_dyn);
            self.inner.dns.extend(dns);
            self
        }
    }

    if_doh_client! {
        /// Add DNS-over-HTTPS services to this client.
        pub fn doh<D: DohService>(mut self, doh: impl IntoIterator<Item = D>) -> Self {
            let doh = doh.into_iter().map(IntoDyn::into_dyn);
            self.inner.doh.extend(doh);
            self
        }
    }

    /// Adds a proxy provider to this client.
    pub fn proxy(mut self, proxy: impl Proxy) -> Self {
        self.inner.proxy = self.inner.proxy.chain([proxy]);
        self
    }
}

/// A [`Transport`] that builds a [`hyper`]-based connector.
#[derive(Debug)]
pub struct Hyper {
    // --- Core ---
    spawner: Option<DynSpawner>,
    resolver: Option<DynResolver>,
    dialer: Option<DynDialer>,

    // --- TLS ---
    anchor: DynTrustAnchor,
    verifier: DynVerifier,
    tls: Option<DynTls>,

    // --- Config ---
    proxy: DynProxy,

    // --- DNS ---
    dns: Vec<DynDnsService>,
    doh: Vec<DynDohService>,
}

impl Default for Hyper {
    fn default() -> Self {
        Self {
            spawner: None,
            resolver: None,
            dialer: None,

            anchor: BaseTrustAnchor.into_dyn(),
            verifier: BaseVerifier.into_dyn(),
            tls: None,

            proxy: BaseProxy.into_dyn(),

            dns: Vec::new(),
            doh: Vec::new(),
        }
    }
}

if_sealed! {
    impl crate::Sealed for Hyper {}
}

impl Transport for Hyper {
    fn build(self, env: &DynEnv) -> Result<DynHttpConnector> {
        // Build the base components.
        let spawner = Self::build_spawner(self.spawner);
        let resolver = Self::build_resolver(self.resolver);
        let dialer = Self::build_dialer(self.dialer);
        let proxy = Self::build_proxy(self.proxy);
        let verifier = Self::build_verifier(self.verifier, env);
        let upgrader = Self::build_upgrader(self.tls, self.anchor, verifier)?;

        // Build the connector.
        let connector = Self::build_connector(&spawner, &resolver, &dialer, &upgrader, &proxy);

        // Extend the resolver with DNS client(s), if any.
        if_dns_client! {
            let resolver = Self::build_dns_resolver(
                self.dns,
                resolver,
                &spawner,
            );
        }

        // Extend the resolver with DNS-over-HTTPS client(s), if any.
        if_doh_client! {
            let resolver = Self::build_doh_resolver(
                self.doh,
                resolver,
                &connector,
            );
        }

        // Rebuild the connector with the new resolver, if necessary.
        if_dns! {
            let connector = Self::build_connector(
                &spawner,
                &resolver,
                &dialer,
                &upgrader,
                &proxy,
            );
        }

        Ok(connector)
    }
}

impl Hyper {
    fn build_spawner(spawner: Option<DynSpawner>) -> DynSpawner {
        spawner.unwrap_or_else(|| {
            if_rt_async! {{
                AsyncSpawner::default().into_dyn()
            } else if_rt_tokio! {{
                TokioSpawner.into_dyn()
            } else {
                compile_error!("a runtime must be enabled")
            }}}
        })
    }

    fn build_resolver(resolver: Option<DynResolver>) -> DynResolver {
        (resolver.unwrap_or_else(|| {
            if_rt_async! {{
                AsyncResolver.into_dyn()
            } else if_rt_tokio! {{
                TokioResolver.into_dyn()
            } else {
                compile_error!("a runtime must be enabled")
            }}}
        })) as _ // see http/common/sender.rs
    }

    fn build_dialer(dialer: Option<DynDialer>) -> DynDialer {
        (dialer.unwrap_or_else(|| {
            if_rt_async! {{
                AsyncDialer.into_dyn()
            } else if_rt_tokio! {{
                TokioDialer.into_dyn()
            } else {
                compile_error!("a runtime must be enabled")
            }}}
        })) as _ // see http/common/sender.rs
    }

    fn build_proxy(proxy: DynProxy) -> DynProxy {
        if_http_proxy! {{
            proxy.chain([
                EnvProxy::external("HTTP_PROXY"),
                EnvProxy::external("http_proxy"),
                EnvProxy::external("HTTPS_PROXY"),
                EnvProxy::external("https_proxy"),
            ])
        } else {
            proxy
        }}
    }

    #[cfg_attr(not(feature = "tls-pinning"), allow(unused_variables))]
    fn build_verifier(verifier: DynVerifier, env: &DynEnv) -> DynVerifier {
        if_tls_pinning! {{
            verifier.chain([TlsPinVerifier::new(env)])
        } else {
            verifier
        }}
    }

    fn build_upgrader(
        tls: Option<DynTls>,
        anchor: DynTrustAnchor,
        verifier: DynVerifier,
    ) -> Result<DynTlsUpgrader> {
        let tls = &tls.unwrap_or_else(|| {
            if_tls_rustls! {{
                RustlsTls.into_dyn()
            } else if_tls_tokio! {{
                TokioTls.into_dyn()
            } else {
                compile_error!("a TLS backend must be enabled")
            }}}
        });

        tls.build_any(anchor, verifier)
    }

    if_dns_client! {
        fn build_dns_resolver(
            svc: Vec<DynDnsService>,
            res: DynResolver,
            exec: &DynSpawner,
        ) -> DynResolver {
            res.layer(svc.into_iter().map(|svc| with_fallback(DnsClient::new(svc, exec))))
        }
    }

    if_doh_client! {
        fn build_doh_resolver(
            svc: Vec<DynDohService>,
            res: DynResolver,
            conn: &DynHttpConnector,
        ) -> DynResolver {
            res.layer(svc.into_iter().map(|svc| with_fallback(DohClient::new(svc, conn))))
        }
    }

    fn build_connector(
        spawner: &DynSpawner,
        resolver: &DynResolver,
        dialer: &DynDialer,
        upgrader: &DynTlsUpgrader,
        proxy: &DynProxy,
    ) -> DynHttpConnector {
        HyperConnector::new(spawner, resolver, dialer, upgrader, proxy).into_dyn()
        // see http/common/sender.rs
    }
}
