use crate::common::prelude::*;
use muon_proc::{autoimpl, derive_dyn};
use std::sync::Arc;

/// A type that provides the URL of a public DNS-over-HTTPS server.
#[autoimpl(for(DynDohService))]
#[derive_dyn(Debug)]
pub trait DohService: Send + Sync + 'static {
    /// Get the DNS-over-HTTPS server.
    fn server(&self) -> Server;
}

/// A dynamic DNS-over-HTTPS service.
pub type DynDohService = Arc<dyn DohService>;

impl<This: DohService> IntoDyn<DynDohService> for This {
    fn into_dyn(self) -> DynDohService {
        Arc::new(self)
    }
}

impl IntoDyn<DynDohService> for &DynDohService {
    fn into_dyn(self) -> DynDohService {
        self.to_owned()
    }
}

/// A macro that generates DNS-over-HTTPS server types.
#[macro_export]
#[doc(hidden)]
macro_rules! impl_doh_service {
    ($($name:ident => $url:expr),* $(,)?) => {$(
        const _: () = {
            impl $crate::dns::DohService for $name {
                fn server(&self) -> $crate::common::Server {
                    $url.parse().unwrap()
                }
            }
        };
    )*};
}

if_doh_client! {
    pub(crate) use impl_doh_service;
}
