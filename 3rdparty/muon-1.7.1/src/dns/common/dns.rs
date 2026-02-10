use crate::common::prelude::*;
use muon_proc::{autoimpl, derive_dyn};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// A type that provides the address of a public DNS server.
#[autoimpl(for(DynDnsService))]
#[derive_dyn(Debug)]
pub trait DnsService: Send + Sync + 'static {
    /// Get the IP address of the DNS server.
    fn ip(&self) -> IpAddr;

    /// Get the port of the DNS server.
    fn port(&self) -> u16 {
        53
    }

    /// Get the socket address of the DNS server.
    fn addr(&self) -> SocketAddr {
        SocketAddr::new(self.ip(), self.port())
    }
}

/// A dynamic DNS service.
pub type DynDnsService = Arc<dyn DnsService>;

impl<This: DnsService> IntoDyn<DynDnsService> for This {
    fn into_dyn(self) -> DynDnsService {
        Arc::new(self)
    }
}

impl IntoDyn<DynDnsService> for &DynDnsService {
    fn into_dyn(self) -> DynDnsService {
        self.to_owned()
    }
}

/// A macro that generates DNS server types.
#[macro_export]
#[doc(hidden)]
macro_rules! impl_dns_service {
    ($($name:ident => ($addr:expr, $port:expr)),* $(,)?) => {$(
        const _: () = {
            impl $crate::dns::DnsService for $name {
                fn ip(&self) -> ::std::net::IpAddr {
                    $addr.into()
                }

                fn port(&self) -> u16 {
                    $port
                }
            }
        };
    )*};
}

if_dns_client! {
    pub(crate) use impl_dns_service;
}
